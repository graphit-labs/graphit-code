//! Per-FILE Arrow spool and the real Lance FTS sidecar of one private graph output.
use crate::{
    graph_writer::IcebugOutput,
    lance_module_store::{LanceModuleStore, ModuleSnapshot, RecordBatchStreamAdapter},
};
use arrow_array::RecordBatch;
use arrow_ipc::{reader::StreamReader, writer::StreamWriter};
use arrow_schema::Schema;
use graphit_core::file_io::SnapshotContext;
use std::{
    fs::File,
    io::{self, BufReader, BufWriter},
    sync::Arc,
};

pub const DIRECTORY: &str = "corpus.lance";
pub use lance_index::scalar::FullTextSearchQuery;

impl crate::CapturedLocalGraph {
    /// Observe the single corpus from this exact admitted immutable graph tree.
    /// Its vector column can have evolved after lexical publication; the domain
    /// reader validates the observed schema and per-row model identities.
    pub async fn open_corpus(
        &self,
        original: &SnapshotContext,
    ) -> io::Result<Option<crate::lance_branch_store::BranchSnapshot>> {
        self.reader.check_request(original)?;
        let Some(node) = self
            .reader
            .nodes()
            .find(|node| node.relative() == std::path::Path::new(DIRECTORY))
        else {
            return Ok(None);
        };
        if node.kind() != graphit_fs::NodeKind::Directory {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let path = self.reader.source_path(original)?.join(DIRECTORY);
        let snapshot = crate::lance_branch_store::BranchSnapshot::capture_observed(
            path.to_str().ok_or(io::ErrorKind::InvalidInput)?,
            None,
            None,
        )
        .await;
        self.reader.revalidate_bindings()?;
        original.check()?;
        snapshot.map(Some).map_err(io::Error::other)
    }
}
pub struct GraphSearchSpool {
    directory: tempfile::TempDir,
    writer: StreamWriter<BufWriter<File>>,
    schema: Arc<Schema>,
    original: Arc<SnapshotContext>,
}
impl GraphSearchSpool {
    pub fn new(schema: Arc<Schema>, original: Arc<SnapshotContext>) -> io::Result<Self> {
        original.check()?;
        let directory = tempfile::tempdir()?;
        let file = File::create(directory.path().join("entities.arrow"))?;
        let mut writer =
            StreamWriter::try_new(BufWriter::new(file), &schema).map_err(io::Error::other)?;
        writer
            .write(&RecordBatch::new_empty(Arc::clone(&schema)))
            .map_err(io::Error::other)?;
        Ok(Self {
            directory,
            writer,
            schema,
            original,
        })
    }
    pub fn push(&mut self, batch: &RecordBatch) -> io::Result<()> {
        self.original.check()?;
        if batch.schema() != self.schema {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.writer.write(batch).map_err(io::Error::other)?;
        self.original.check()
    }
    /// Complete real Main and code FTS indices before the graph's single Copy publication.
    /// The SDK corpus is private to this output; this API cannot mutate a published dataset.
    pub async fn finish(
        mut self,
        output: &IcebugOutput,
        columns: &[&str],
    ) -> io::Result<ModuleSnapshot> {
        self.original.check()?;
        if !Arc::ptr_eq(output.original(), &self.original) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.writer.finish().map_err(io::Error::other)?;
        let mut file = self.writer.into_inner().map_err(io::Error::other)?;
        std::io::Write::flush(&mut file)?;
        drop(file);
        let reader = StreamReader::try_new(
            BufReader::new(File::open(self.directory.path().join("entities.arrow"))?),
            None,
        )
        .map_err(io::Error::other)?;
        let original = Arc::clone(&self.original);
        let batches = reader.map(move |batch| {
            original
                .check()
                .map_err(crate::lance_module_store::DataFusionError::from)?;
            batch.map_err(crate::lance_module_store::DataFusionError::from)
        });
        let stream = Box::pin(RecordBatchStreamAdapter::new(
            Arc::clone(&self.schema),
            futures::stream::iter(batches),
        ));
        let path = output.directory().join(DIRECTORY);
        let uri = path.to_str().ok_or(io::ErrorKind::InvalidInput)?;
        let prepared =
            LanceModuleStore::prepare_create_stream(uri, Arc::clone(&self.schema), stream)
                .await
                .map_err(io::Error::other)?;
        self.original.check()?;
        let (store, _) = prepared.commit().await.map_err(io::Error::other)?;
        self.original.check()?;
        let snapshot = store
            .snapshot()
            .build_code_fts(columns, &self.original)
            .await
            .map_err(io::Error::other)?;
        self.original.check()?;
        Ok(snapshot)
    }
}
