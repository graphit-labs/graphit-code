//! FILE publication into the project's single branch-aware search corpus.
use super::*;
use futures_util::TryStreamExt;
use graphit_ast::corpus;
use graphit_data::lance_branch_store::{AST_INDEX_BATCH, cleanup_ast_corpus};

pub(crate) struct ProgressivePublication<'a> {
    cohort: CapturedCohort,
    project: ProjectCapture<'a>,
    original: Arc<SnapshotContext>,
    path: PathBuf,
    directory: CapturedDocumentDirectory,
    files_since_maintenance: usize,
}

impl<'a> ProgressivePublication<'a> {
    pub async fn open(
        installation: ScopeCapture<'a>,
        project: ProjectCapture<'a>,
        native: NativeDocumentFileLimits,
        original: Arc<SnapshotContext>,
        registry: Arc<LiveAstRegistry>,
        recheck: &impl Fn() -> io::Result<()>,
    ) -> io::Result<Self> {
        recheck()?;
        let cohort = registry
            .capture_cohort(installation, project, native, &original, true)?
            .ok_or(io::ErrorKind::NotFound)?;
        let fence = write_fence(&cohort.slot.fence, &original).await?;
        let relative = graphit_core::local_corpus::resolve(
            project.scope.live,
            project.version.version(),
            "ast",
            corpus::DIRECTORY,
            native,
            &original,
        )?;
        let mapped = relative != Path::new("ast").join(corpus::DIRECTORY);
        let path = project.scope.live.runtime_root().join(relative);
        let directory = path.parent().ok_or(io::ErrorKind::InvalidData)?;
        let parent = CapturedDocumentDirectory::capture(directory, native, &original)?;
        if !path.try_exists()? {
            if mapped {
                return Err(io::ErrorKind::NotFound.into());
            }
            let stage = tempfile::Builder::new()
                .prefix(".corpus-create-")
                .tempdir_in(directory)?;
            let prepared = LanceModuleStore::prepare_create(
                stage.path().to_str().ok_or(io::ErrorKind::InvalidData)?,
                corpus::schema(),
                vec![RecordBatch::new_empty(corpus::schema())],
            )
            .await
            .map_err(err)?;
            #[cfg(test)]
            {
                let gate = registry.1.lock().unwrap().clone();
                if let Some(gate) = gate {
                    gate.wait().await;
                }
            }
            recheck()?;
            parent.revalidate(&original)?;
            prepared.commit().await.map_err(err)?;
            let initial = BranchSnapshot::capture_observed(
                stage.path().to_str().ok_or(io::ErrorKind::InvalidData)?,
                None,
                None,
            )
            .await
            .map_err(err)?;
            if let Some(indices) = initial.prepare_ast_indices().await? {
                recheck()?;
                indices.commit().await.map_err(err)?;
            }
            recheck()?;
            parent.revalidate(&original)?;
            match graphit_fs::rename_exclusive(stage.path(), &path) {
                Ok(()) => (),
                // Another version may have installed the same empty Main while
                // this version held only its own graph fence. Reopen and validate
                // that actual winner below; no schema/path fallback is accepted.
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error),
            }
        }
        let directory = CapturedDocumentDirectory::capture(&path, native, &original)?;
        let mut search = snapshot(&path, &directory, project, &original).await?;
        if search.inherited_target().is_some() {
            recheck()?;
            search = search.materialize().await.map_err(err)?;
        }
        drop(search);
        drop(fence);
        Ok(Self {
            cohort,
            project,
            original,
            path,
            directory,
            files_since_maintenance: 0,
        })
    }

    pub(in crate::foreground_ast_sources) async fn capture_index(
        &self,
    ) -> io::Result<Arc<LiveIndex>> {
        self.cohort.directory.revalidate(&self.original)?;
        self.cohort
            .capture_index(&self.original, true)
            .await?
            .ok_or(io::ErrorKind::NotFound.into())
    }

    async fn capture_search(&self) -> io::Result<BranchSnapshot> {
        snapshot(&self.path, &self.directory, self.project, &self.original).await
    }

    pub async fn retain_paths(
        &mut self,
        retained: &BTreeSet<&str>,
        recheck: &impl Fn() -> io::Result<()>,
    ) -> io::Result<()> {
        let _fence = write_fence(&self.cohort.slot.fence, &self.original).await?;
        let search = self.capture_search().await?;
        let deleted = if retained.is_empty() {
            lit(true)
        } else {
            col("path").in_list(retained.iter().map(|path| lit(*path)).collect(), true)
        };
        let mut scan = search.scan();
        scan.project(&["path"]).map_err(err)?;
        scan.filter_expr(deleted.clone());
        scan.limit(Some(1), None).map_err(err)?;
        let mut stream = scan.try_into_stream().await.map_err(err)?;
        let present = stream
            .try_next()
            .await
            .map_err(err)?
            .is_some_and(|batch| batch.num_rows() != 0);
        drop(stream);
        if present {
            let prepared = search.prepare_delete_expr(deleted).await.map_err(err)?;
            recheck()?;
            self.directory.revalidate(&self.original)?;
            drop(prepared.commit().await.map_err(err)?);
            recheck()?;
            self.directory.revalidate(&self.original)?;
        }
        Ok(())
    }

    pub async fn publish_file(
        &mut self,
        file: FilePublication,
        recheck: &impl Fn() -> io::Result<()>,
    ) -> io::Result<u64> {
        let _fence = write_fence(&self.cohort.slot.fence, &self.original).await?;
        recheck()?;
        let search = self.capture_search().await?;
        let previous = file_rows(&search, &file.path, &self.original).await?;
        let rows = corpus::encode_file(
            Arc::new(search.schema().clone()),
            concat(graphit_ast::entity_search::schema(), file.search.clone())?,
            concat(
                graphit_ast::native_nn::native_nn_projection_schema(),
                file.projections.clone(),
            )?,
            Some(&previous),
            &self.original,
        )?;
        let prepared = prepare_file(&search, &file.path, rows).await?;
        recheck()?;
        self.directory.revalidate(&self.original)?;
        drop(prepared.commit().await.map_err(err)?);
        // Text becomes queryable before the independent graph publication.
        let index = self.capture_index().await?;
        let revision = index.graph.replace_file(input(&file), &self.original)?;
        drop(index);
        recheck()?;
        self.directory.revalidate(&self.original)?;
        self.cohort.directory.revalidate(&self.original)?;
        drop(search);
        drop(_fence);
        self.after_file(recheck).await?;
        Ok(revision)
    }

    pub async fn enrich_file(
        &mut self,
        file: FilePublication,
        expected_revision: u64,
        recheck: &impl Fn() -> io::Result<()>,
    ) -> io::Result<Option<u64>> {
        let _fence = write_fence(&self.cohort.slot.fence, &self.original).await?;
        recheck()?;
        self.directory.revalidate(&self.original)?;
        let index = self.capture_index().await?;
        if !index
            .graph
            .file_observation(&file.path, &self.original)?
            .is_some_and(|row| {
                row.file_revision == expected_revision && row.source_sha256 == file.source_sha256
            })
        {
            return Ok(None);
        }
        let search = self.capture_search().await?;
        let previous = file_rows(&search, &file.path, &self.original).await?;
        let old_lexical = lexical(&previous)?;
        let lexical = union(
            old_lexical,
            concat(graphit_ast::entity_search::schema(), file.search.clone())?,
        )?;
        let prepared = union(
            corpus::projections(&previous, &self.original)?,
            concat(
                graphit_ast::native_nn::native_nn_projection_schema(),
                file.projections.clone(),
            )?,
        )?;
        let rows = corpus::encode_file(
            Arc::new(search.schema().clone()),
            lexical,
            prepared,
            Some(&previous),
            &self.original,
        )?;
        let prepared = prepare_file(&search, &file.path, rows).await?;
        let revision =
            match index
                .graph
                .enrich_file(input(&file), expected_revision, &self.original)
            {
                Ok(version) => version,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                Err(error) => return Err(error),
            };
        drop(index);
        recheck()?;
        self.directory.revalidate(&self.original)?;
        drop(prepared.commit().await.map_err(err)?);
        recheck()?;
        self.directory.revalidate(&self.original)?;
        drop(search);
        drop(_fence);
        self.after_file(recheck).await?;
        Ok(Some(revision))
    }

    pub async fn remove_file(
        &mut self,
        path: &str,
        recheck: &impl Fn() -> io::Result<()>,
    ) -> io::Result<u64> {
        let _fence = write_fence(&self.cohort.slot.fence, &self.original).await?;
        recheck()?;
        let search = self.capture_search().await?;
        let prepared = search
            .prepare_delete_expr(col("path").eq(lit(path)))
            .await
            .map_err(err)?;
        recheck()?;
        self.directory.revalidate(&self.original)?;
        drop(prepared.commit().await.map_err(err)?);
        let index = self.capture_index().await?;
        let revision = index.graph.remove_file(path, &self.original)?;
        drop(index);
        recheck()?;
        self.directory.revalidate(&self.original)?;
        drop(search);
        drop(_fence);
        self.after_file(recheck).await?;
        Ok(revision)
    }

    async fn after_file(&mut self, recheck: &impl Fn() -> io::Result<()>) -> io::Result<()> {
        self.files_since_maintenance += 1;
        if self.files_since_maintenance >= AST_INDEX_BATCH {
            self.maintain_indices(recheck).await?;
        }
        Ok(())
    }

    pub(crate) async fn maintain_indices(
        &mut self,
        recheck: &impl Fn() -> io::Result<()>,
    ) -> io::Result<()> {
        let _fence = write_fence(&self.cohort.slot.fence, &self.original).await?;
        let search = self.capture_search().await?;
        let check = || {
            recheck()?;
            self.directory.revalidate(&self.original)
        };
        if let Err(error) = search.maintain_ast_indices(&self.original, &check).await {
            check()?;
            tracing::warn!(%error, "AST index maintenance deferred");
        }
        self.files_since_maintenance = 0;
        Ok(())
    }

    pub(crate) async fn clean_history(
        self,
        recheck: &impl Fn() -> io::Result<()>,
    ) -> io::Result<()> {
        let search = self.capture_search().await?;
        clean_corpus_history(&self.path, &self.directory, search, &self.original, recheck).await
    }
}

pub(crate) async fn clean_corpus_history(
    path: &Path,
    directory: &CapturedDocumentDirectory,
    search: BranchSnapshot,
    original: &SnapshotContext,
    recheck: &impl Fn() -> io::Result<()>,
) -> io::Result<()> {
    let due = search.ast_history_due().await;
    drop(search);
    recheck()?;
    directory.revalidate(original)?;
    let result = match due {
        Ok(true) => cleanup_ast_corpus(path, original).await.map(|_| ()),
        Ok(false) => Ok(()),
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        recheck()?;
        tracing::warn!(%error, "AST obsolete version cleanup deferred");
    }
    recheck()
}

fn input(file: &FilePublication) -> graphit_ast::live_graph::LiveFileInput<'_> {
    graphit_ast::live_graph::LiveFileInput {
        path: &file.path,
        source_sha256: file.source_sha256,
        raw_version: file.raw_version,
        nodes: &file.nodes,
        edges: &file.edges,
        definitions: &file.definitions,
        references: &file.references,
        scip_document: file.scip_document.as_deref(),
        pending_compiler: file.pending_compiler.as_deref(),
        scip_compiler: file.scip_compiler.as_deref(),
        oracle: &file.oracle,
        oracle_file: file.oracle_file.as_ref(),
    }
}

pub(crate) async fn snapshot(
    path: &Path,
    directory: &CapturedDocumentDirectory,
    project: ProjectCapture<'_>,
    original: &SnapshotContext,
) -> io::Result<BranchSnapshot> {
    original.check()?;
    directory.revalidate(original)?;
    let branch = project.version.lance_branch();
    let observed = BranchSnapshot::capture_ast_corpus(
        path,
        (!branch.is_empty()).then_some(branch.as_str()),
        original,
    )
    .await?;
    directory.revalidate(original)?;
    original.check()?;
    corpus::validate_schema(observed.schema())?;
    Ok(observed)
}

pub(crate) async fn file_rows(
    snapshot: &BranchSnapshot,
    path: &str,
    original: &SnapshotContext,
) -> io::Result<RecordBatch> {
    let mut scan = snapshot.scan();
    scan.filter_expr(col("path").eq(lit(path)));
    let mut stream = scan.try_into_stream().await.map_err(err)?;
    let mut rows = Vec::new();
    while let Some(batch) = stream.try_next().await.map_err(err)? {
        original.check()?;
        rows.push(batch);
    }
    concat(Arc::new(snapshot.schema().clone()), rows)
}

async fn prepare_file(
    snapshot: &BranchSnapshot,
    path: &str,
    batch: RecordBatch,
) -> io::Result<graphit_data::lance_branch_store::PreparedDelta> {
    let stream = Box::pin(RecordBatchStreamAdapter::new(
        batch.schema(),
        tokio_stream::iter([Ok(batch)]),
    ));
    snapshot
        .prepare_replace_paths_stream(
            &[path.to_owned()],
            stream,
            vec!["uid".into(), "data_key".into()],
        )
        .await
        .map_err(err)
}

fn lexical(batch: &RecordBatch) -> io::Result<RecordBatch> {
    let schema = graphit_ast::entity_search::schema();
    let arrays = schema
        .fields()
        .iter()
        .map(|field| {
            batch
                .column_by_name(field.name())
                .cloned()
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
        })
        .collect::<io::Result<Vec<_>>>()?;
    RecordBatch::try_new(schema, arrays).map_err(err)
}

fn union(old: RecordBatch, new: RecordBatch) -> io::Result<RecordBatch> {
    use arrow_array::{Array, BooleanArray};
    let keys = |batch: &RecordBatch| -> io::Result<Vec<(String, String)>> {
        let col = |name| {
            batch
                .column_by_name(name)
                .and_then(|a| a.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
        };
        let (uid, key) = (col("uid")?, col("data_key")?);
        if uid.null_count() != 0 || key.null_count() != 0 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok((0..batch.num_rows())
            .map(|row| (uid.value(row).to_owned(), key.value(row).to_owned()))
            .collect())
    };
    let present = keys(&new)?.into_iter().collect::<BTreeSet<_>>();
    let mask = BooleanArray::from(
        keys(&old)?
            .iter()
            .map(|key| !present.contains(key))
            .collect::<Vec<_>>(),
    );
    let old = arrow_select::filter::filter_record_batch(&old, &mask).map_err(err)?;
    concat(new.schema(), vec![old, new])
}

fn concat(schema: Arc<arrow_schema::Schema>, batches: Vec<RecordBatch>) -> io::Result<RecordBatch> {
    if batches.is_empty() {
        return Ok(RecordBatch::new_empty(schema));
    }
    arrow_select::concat::concat_batches(&schema, &batches).map_err(err)
}
