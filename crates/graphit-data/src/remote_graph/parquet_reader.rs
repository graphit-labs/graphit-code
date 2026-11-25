use super::{RemoteGraphError, Result, wait};
use crate::{
    DataReadError, LocalGraphLimits,
    parquet::{
        Chunk, LookupKey, Scalar, Shape, Work, slice_row_groups, validate, validate_page,
        validate_shape,
    },
    protocol,
};
use arrow_array_parquet::{RecordBatch, new_null_array};
use arrow_schema_parquet::{DataType, Field, Schema};
use bytes::Bytes;
use futures::{FutureExt, StreamExt, future::BoxFuture, stream::BoxStream};
use graphit_core::file_io::SnapshotContext;
use object_store::{GetOptions, GetRange, ObjectMeta, ObjectStore, ObjectStoreExt, path::Path};
use parquet::{
    arrow::{
        ParquetRecordBatchStreamBuilder, ProjectionMask,
        arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions, RowSelection},
        async_reader::AsyncFileReader,
    },
    errors::ParquetError,
    file::metadata::{ParquetMetaData, ParquetMetaDataReader},
};
use std::{collections::BTreeMap, ops::Range, sync::Arc};

pub(super) struct Object {
    store: Arc<dyn ObjectStore>,
    meta: ObjectMeta,
}
impl Object {
    pub(super) async fn open(
        store: Arc<dyn ObjectStore>,
        path: Path,
        original: &SnapshotContext,
    ) -> Result<Self> {
        let meta = wait(original, async { Ok(store.head(&path).await?) }).await?;
        Ok(Self { store, meta })
    }
    pub(super) fn size(&self) -> u64 {
        self.meta.size
    }
    pub(super) async fn range(
        &self,
        range: Range<u64>,
        original: &SnapshotContext,
    ) -> Result<Bytes> {
        if range.start > range.end || range.end > self.size() {
            return Err(DataReadError::InvalidParquet("object range").into());
        }
        let expected = range.end - range.start;
        let bytes = wait(original, async {
            let options = GetOptions {
                range: Some(GetRange::Bounded(range)),
                if_match: self.meta.e_tag.clone(),
                version: self.meta.version.clone(),
                ..Default::default()
            };
            Ok(self
                .store
                .get_opts(&self.meta.location, options)
                .await?
                .bytes()
                .await?)
        })
        .await?;
        if bytes.len() as u64 != expected {
            return Err(DataReadError::InvalidParquet("object range length").into());
        }
        Ok(bytes)
    }
}

#[derive(Clone)]
pub(super) struct Table {
    shards: Vec<(u64, Box<Table>)>,
    pub(super) object: Arc<Object>,
    metadata: ArrowReaderMetadata,
    chunks: Arc<Vec<Chunk>>,
    row_groups: Vec<usize>,
    pub(super) row_offset: u64,
    pub(super) fields: Vec<(String, Scalar)>,
    pub(super) rows: u64,
}
impl Table {
    pub(super) async fn open_nodes(
        store: Arc<dyn ObjectStore>,
        prefix: &Path,
        node: &crate::manifest::NodeTable,
        limits: LocalGraphLimits,
        original: &SnapshotContext,
        captured: &mut BTreeMap<Path, Self>,
    ) -> Result<Self> {
        let mut shards = Vec::new();
        for shard in &node.shards {
            let part = Self::open(
                Arc::clone(&store),
                super::object_path(prefix, &shard.file),
                shard.row_offset,
                shard.rows,
                Shape::Nodes(&node.columns),
                limits,
                original,
                captured,
            )
            .await?;
            if !part
                .fields
                .iter()
                .any(|(name, _)| name == &node.primary_key)
                && shard.rows != 0
            {
                return Err(DataReadError::InvalidParquet("shard primary key").into());
            }
            shards.push((shard.row_base, Box::new(part)));
        }
        let mut table = shards
            .first()
            .ok_or(DataReadError::InvalidManifest("node shards"))?
            .1
            .as_ref()
            .clone();
        table.fields = node
            .columns
            .iter()
            .map(|column| {
                Ok((
                    column.name.clone(),
                    match column.kind.as_str() {
                        "STRING" => Scalar::String,
                        "INT64" => Scalar::Int64,
                        "BOOL" => Scalar::Bool,
                        _ => return Err(DataReadError::InvalidParquet("manifest type")),
                    },
                ))
            })
            .collect::<crate::error::Result<_>>()?;
        table.rows = node.rows;
        table.shards = shards;
        Ok(table)
    }
    pub(super) fn shard_object(&self, index: usize) -> Result<Arc<Object>> {
        self.shards
            .get(index)
            .map(|(_, part)| Arc::clone(&part.object))
            .ok_or_else(|| DataReadError::InvalidManifest("node shard object").into())
    }
    pub(super) fn contains(&self, offset: u64) -> bool {
        if self.shards.is_empty() {
            return offset < self.rows;
        }
        self.shards
            .iter()
            .any(|(base, part)| offset >= *base && offset - *base < part.rows)
    }
    pub(super) fn offset(&self, mut dense: u64) -> Result<u64> {
        if self.shards.is_empty() && dense < self.rows {
            return Ok(dense);
        }
        for (base, part) in &self.shards {
            if dense < part.rows {
                return base
                    .checked_add(dense)
                    .ok_or_else(|| DataReadError::InvalidParquet("node row ordinal").into());
            }
            dense -= part.rows;
        }
        Err(DataReadError::InvalidParquet("node row ordinal").into())
    }
    fn dense(&self, offset: u64) -> Result<u64> {
        if self.shards.is_empty() && offset < self.rows {
            return Ok(offset);
        }
        let mut dense = 0u64;
        for (base, part) in &self.shards {
            if offset >= *base && offset - *base < part.rows {
                return dense
                    .checked_add(offset - *base)
                    .ok_or_else(|| DataReadError::InvalidParquet("node dense ordinal").into());
            }
            dense = dense
                .checked_add(part.rows)
                .ok_or(DataReadError::InvalidParquet("node dense extent"))?;
        }
        Err(DataReadError::InvalidParquet("node row offset").into())
    }
    pub(super) async fn open(
        store: Arc<dyn ObjectStore>,
        path: Path,
        row_offset: u64,
        rows: u64,
        shape: Shape<'_>,
        limits: LocalGraphLimits,
        original: &SnapshotContext,
        captured: &mut BTreeMap<Path, Self>,
    ) -> Result<Self> {
        if let Some(physical) = captured.get(&path) {
            validate_shape(&physical.fields, shape)?;
            let metadata = physical.metadata.metadata();
            let groups = slice_row_groups(
                metadata.file_metadata().num_rows(),
                metadata.row_groups().iter().map(|group| group.num_rows()),
                row_offset,
                rows,
                shape,
            )?;
            let mut table = physical.clone();
            table.row_offset = row_offset;
            table.rows = rows;
            table.row_groups = groups;
            return Ok(table);
        }
        let location = path.clone();
        let object = Arc::new(Object::open(store, path, original).await?);
        let size = object.size();
        if size < 12 || object.range(0..4, original).await?.as_ref() != b"PAR1" {
            return Err(DataReadError::InvalidParquet("magic").into());
        }
        let trailer = object.range(size - 8..size, original).await?;
        if &trailer[4..] != b"PAR1" {
            return Err(DataReadError::InvalidParquet("trailer magic").into());
        }
        let footer_size = u32::from_le_bytes(trailer[..4].try_into().expect("four bytes"));
        crate::bounded(
            footer_size as usize,
            limits.max_footer_bytes,
            "footer bytes",
        )?;
        let start = (size - 8)
            .checked_sub(u64::from(footer_size))
            .filter(|start| *start >= 4)
            .ok_or(DataReadError::InvalidParquet("footer span"))?;
        let footer = object.range(start..size - 8, original).await?;
        let parsed = protocol::footer(&footer, limits)?;
        let physical_rows =
            u64::try_from(parsed.num_rows).map_err(|_| DataReadError::InvalidParquet("rows"))?;
        let validated = validate(&parsed, physical_rows, shape, start, limits)?;
        let row_groups = slice_row_groups(
            parsed.num_rows,
            parsed.row_groups.iter().map(|group| group.num_rows),
            row_offset,
            rows,
            shape,
        )?;
        let metadata = ArrowReaderMetadata::try_new(
            Arc::new(ParquetMetaDataReader::decode_metadata(&footer)?),
            ArrowReaderOptions::new().with_skip_arrow_metadata(true),
        )?;
        original.check()?;
        let table = Self {
            shards: Vec::new(),
            object,
            metadata,
            chunks: Arc::new(validated.chunks),
            row_groups,
            row_offset,
            fields: validated.fields,
            rows,
        };
        let mut physical = table.clone();
        physical.row_offset = 0;
        physical.rows = physical_rows;
        physical.row_groups = (0..physical.metadata.metadata().num_row_groups()).collect();
        captured.insert(location, physical);
        Ok(table)
    }
    pub(super) fn select_rows(&self, rows: &[u64]) -> Result<RowSelection> {
        if rows.iter().any(|row| !self.contains(*row))
            || rows.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(DataReadError::InvalidParquet("selected rows").into());
        }
        let rows = rows
            .iter()
            .map(|row| self.dense(*row))
            .collect::<Result<Vec<_>>>()?;
        let total = usize::try_from(self.rows)
            .map_err(|_| DataReadError::LimitExceeded("row selection"))?;
        let ranges = rows
            .iter()
            .map(|row| {
                let start = usize::try_from(*row)
                    .map_err(|_| DataReadError::LimitExceeded("row selection"))?;
                let end = start
                    .checked_add(1)
                    .ok_or(DataReadError::LimitExceeded("row selection"))?;
                Ok(start..end)
            })
            .collect::<crate::error::Result<Vec<_>>>()?;
        Ok(RowSelection::from_consecutive_ranges(
            ranges.into_iter(),
            total,
        ))
    }
    /// Dense row ranges of captured groups which can contain an exact key.
    /// This is metadata pruning, never a claim that a key is present or unique.
    pub(super) fn key_ranges(&self, name: &str, keys: &[LookupKey<'_>]) -> Result<Vec<Range<u64>>> {
        if keys.is_empty() || self.rows == 0 {
            return Ok(Vec::new());
        }
        if !self.shards.is_empty() {
            let mut ranges = Vec::new();
            let mut dense = 0u64;
            for (_, part) in &self.shards {
                for range in part.key_ranges(name, keys)? {
                    ranges.push(
                        dense
                            .checked_add(range.start)
                            .ok_or(DataReadError::InvalidParquet("node row ordinal"))?
                            ..dense
                                .checked_add(range.end)
                                .ok_or(DataReadError::InvalidParquet("node row ordinal"))?,
                    );
                }
                dense = dense
                    .checked_add(part.rows)
                    .ok_or(DataReadError::InvalidParquet("node row ordinal"))?;
            }
            return Ok(ranges);
        }
        let column = self
            .fields
            .iter()
            .position(|(field, _)| field == name)
            .ok_or(DataReadError::InvalidParquet("lookup column"))?;
        let mut ranges = Vec::new();
        let mut offset = 0u64;
        for (index, group) in self.metadata.metadata().row_groups().iter().enumerate() {
            let count = u64::try_from(group.num_rows())
                .map_err(|_| DataReadError::InvalidParquet("group rows"))?;
            let end = offset
                .checked_add(count)
                .ok_or(DataReadError::InvalidParquet("group rows"))?;
            let mut matched = false;
            if self.row_groups.binary_search(&index).is_ok() && count != 0 {
                for key in keys {
                    matched |= key.may_match(group.column(column).statistics())?;
                }
            }
            if matched {
                ranges.push(
                    offset
                        .checked_sub(self.row_offset)
                        .ok_or(DataReadError::InvalidParquet("row slice boundary"))?
                        ..end
                            .checked_sub(self.row_offset)
                            .ok_or(DataReadError::InvalidParquet("row slice boundary"))?,
                );
            }
            offset = end;
        }
        Ok(ranges)
    }
    pub(super) fn select_ranges(&self, ranges: &[Range<u64>]) -> Result<RowSelection> {
        let total = usize::try_from(self.rows)
            .map_err(|_| DataReadError::LimitExceeded("row selection"))?;
        let ranges = ranges
            .iter()
            .map(|range| {
                if range.start > range.end || range.end > self.rows {
                    return Err(DataReadError::InvalidParquet("selected rows"));
                }
                Ok(usize::try_from(range.start)
                    .map_err(|_| DataReadError::LimitExceeded("row selection"))?
                    ..usize::try_from(range.end)
                        .map_err(|_| DataReadError::LimitExceeded("row selection"))?)
            })
            .collect::<crate::error::Result<Vec<_>>>()?;
        if ranges.windows(2).any(|pair| pair[0].end > pair[1].start) {
            return Err(DataReadError::InvalidParquet("selected rows").into());
        }
        Ok(RowSelection::from_consecutive_ranges(
            ranges.into_iter(),
            total,
        ))
    }
    pub(super) fn stream(
        &self,
        names: &[&str],
        selected: Option<RowSelection>,
        limits: LocalGraphLimits,
        batch_size: usize,
        work: Arc<Work>,
        original: Arc<SnapshotContext>,
    ) -> Result<BoxStream<'static, parquet::errors::Result<RecordBatch>>> {
        original.check()?;
        if !self.shards.is_empty() {
            let schema = Arc::new(Schema::new(
                names
                    .iter()
                    .map(|name| {
                        let scalar = self
                            .fields
                            .iter()
                            .find(|(field, _)| field == name)
                            .ok_or(DataReadError::InvalidParquet("projection"))?
                            .1;
                        let kind = match scalar {
                            Scalar::String => DataType::Utf8,
                            Scalar::Int64 => DataType::Int64,
                            Scalar::Bool => DataType::Boolean,
                            Scalar::UInt64 => DataType::UInt64,
                        };
                        Ok(Field::new(*name, kind, true))
                    })
                    .collect::<crate::error::Result<Vec<_>>>()?,
            ));
            let mut remaining = selected;
            let mut streams = Vec::new();
            for (_, part) in &self.shards {
                let count = usize::try_from(part.rows)
                    .map_err(|_| DataReadError::LimitExceeded("row selection"))?;
                let selected = remaining
                    .as_mut()
                    .map(|selection| selection.split_off(count));
                let mut physical = names
                    .iter()
                    .copied()
                    .filter(|name| part.fields.iter().any(|(field, _)| field == name))
                    .collect::<Vec<_>>();
                if physical.is_empty()
                    && let Some((name, _)) = part.fields.first()
                {
                    physical.push(name);
                }
                let stream = part.stream(
                    &physical,
                    selected,
                    limits,
                    batch_size,
                    Arc::clone(&work),
                    Arc::clone(&original),
                )?;
                let projected = Arc::clone(&schema);
                streams.push(
                    stream
                        .map(move |batch| {
                            let batch = batch?;
                            let arrays = projected
                                .fields()
                                .iter()
                                .map(|field| {
                                    batch.column_by_name(field.name()).cloned().unwrap_or_else(
                                        || new_null_array(field.data_type(), batch.num_rows()),
                                    )
                                })
                                .collect();
                            RecordBatch::try_new(Arc::clone(&projected), arrays).map_err(Into::into)
                        })
                        .boxed(),
                );
            }
            return Ok(futures::stream::iter(streams).flatten().boxed());
        }
        let indices = names
            .iter()
            .map(|name| {
                self.fields
                    .iter()
                    .position(|(field, _)| field == name)
                    .ok_or(DataReadError::InvalidParquet("projection"))
            })
            .collect::<crate::error::Result<Vec<_>>>()?;
        let projection = ProjectionMask::roots(
            self.metadata.metadata().file_metadata().schema_descr(),
            indices,
        );
        let reader = Reader {
            object: Arc::clone(&self.object),
            metadata: Arc::clone(self.metadata.metadata()),
            chunks: Arc::clone(&self.chunks),
            limits,
            work,
            original,
        };
        let mut builder =
            ParquetRecordBatchStreamBuilder::new_with_metadata(reader, self.metadata.clone())
                .with_row_groups(self.row_groups.clone())
                .with_projection(projection)
                .with_batch_size(batch_size);
        let count = usize::try_from(self.rows)
            .map_err(|_| DataReadError::LimitExceeded("row selection"))?;
        builder = builder.with_row_selection(selected.unwrap_or_else(|| {
            RowSelection::from_consecutive_ranges(std::iter::once(0..count), count)
        }));
        Ok(builder.build()?.boxed())
    }
}

pub(super) struct Reader {
    pub(super) object: Arc<Object>,
    metadata: Arc<ParquetMetaData>,
    chunks: Arc<Vec<Chunk>>,
    limits: LocalGraphLimits,
    work: Arc<Work>,
    original: Arc<SnapshotContext>,
}
impl Reader {
    async fn read(&self, range: Range<u64>) -> Result<Bytes> {
        let chunk = self
            .chunks
            .iter()
            .find(|chunk| chunk.start == range.start && chunk.end == range.end)
            .ok_or(DataReadError::InvalidParquet("SDK column range"))?;
        let bytes = self.object.range(range, &self.original).await?;
        let mut offset = 0;
        let mut values = 0u64;
        let mut pages = 0;
        while offset < bytes.len() {
            self.original.check()?;
            pages += 1;
            crate::bounded(pages, self.limits.max_pages, "chunk pages")?;
            self.work.page()?;
            let end = offset
                .saturating_add(self.limits.max_header_bytes)
                .min(bytes.len());
            let (header, header_bytes) = protocol::header(&bytes[offset..end], self.limits)?;
            let (compressed, _, count) = validate_page(&header, self.limits)?;
            offset = offset
                .checked_add(header_bytes)
                .and_then(|offset| offset.checked_add(compressed))
                .filter(|offset| *offset <= bytes.len())
                .ok_or(DataReadError::InvalidParquet("page span"))?;
            values = values
                .checked_add(count)
                .filter(|count| *count <= chunk.values)
                .ok_or(DataReadError::InvalidParquet("page value count"))?;
        }
        if values != chunk.values {
            return Err(DataReadError::InvalidParquet("page value total").into());
        }
        self.original.check()?;
        Ok(bytes)
    }
}
fn sdk_error(error: RemoteGraphError) -> ParquetError {
    ParquetError::External(Box::new(error))
}
impl AsyncFileReader for Reader {
    fn get_bytes(&mut self, range: Range<u64>) -> BoxFuture<'_, parquet::errors::Result<Bytes>> {
        async move { self.read(range).await.map_err(sdk_error) }.boxed()
    }
    fn get_byte_ranges(
        &mut self,
        ranges: Vec<Range<u64>>,
    ) -> BoxFuture<'_, parquet::errors::Result<Vec<Bytes>>> {
        async move {
            let mut bytes = Vec::with_capacity(ranges.len());
            for range in ranges {
                bytes.push(self.read(range).await.map_err(sdk_error)?);
            }
            Ok(bytes)
        }
        .boxed()
    }
    fn get_metadata<'a>(
        &'a mut self,
        _options: Option<&'a ArrowReaderOptions>,
    ) -> BoxFuture<'a, parquet::errors::Result<Arc<ParquetMetaData>>> {
        async move {
            self.original
                .check()
                .map_err(|error| sdk_error(error.into()))?;
            Ok(Arc::clone(&self.metadata))
        }
        .boxed()
    }
}
