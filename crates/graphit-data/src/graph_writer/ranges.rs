//! Physical row slices over canonical Parquet, independent of graph identity.
use super::*;
use bytes::Bytes;
use parquet::{
    arrow::arrow_reader::{
        ParquetRecordBatchReader, ParquetRecordBatchReaderBuilder, RowSelection,
    },
    column::writer::ColumnCloseResult,
    file::{
        metadata::{ParquetMetaData, ParquetMetaDataReader},
        reader::{ChunkReader, Length},
    },
};
use std::{
    io::Read,
    ops::{Bound, Range, RangeBounds},
};

fn bounds(rows: impl RangeBounds<u64>, total: u64) -> io::Result<Range<u64>> {
    let start = match rows.start_bound() {
        Bound::Included(value) => *value,
        Bound::Excluded(value) => value
            .checked_add(1)
            .ok_or_else(|| invalid("row range overflow"))?,
        Bound::Unbounded => 0,
    };
    let end = match rows.end_bound() {
        Bound::Included(value) => value
            .checked_add(1)
            .ok_or_else(|| invalid("row range overflow"))?,
        Bound::Excluded(value) => *value,
        Bound::Unbounded => total,
    };
    if start > end || end > total {
        return Err(invalid("row range outside Parquet"));
    }
    Ok(start..end)
}

pub(super) fn read_rows(
    path: &Path,
    rows: impl RangeBounds<u64>,
    original: &SnapshotContext,
) -> io::Result<ParquetRecordBatchReader> {
    original.check()?;
    let builder =
        ParquetRecordBatchReaderBuilder::try_new(File::open(path)?).map_err(io::Error::other)?;
    let total = u64::try_from(builder.metadata().file_metadata().num_rows())
        .map_err(|_| invalid("negative Parquet row count"))?;
    let range = bounds(rows, total)?;
    let mut groups = Vec::new();
    let mut position = 0u64;
    let mut base = range.start;
    let mut selected_rows = 0u64;
    for (index, group) in builder.metadata().row_groups().iter().enumerate() {
        original.check()?;
        let rows =
            u64::try_from(group.num_rows()).map_err(|_| invalid("negative Parquet group rows"))?;
        let end = position
            .checked_add(rows)
            .ok_or_else(|| invalid("Parquet row count overflow"))?;
        if range.start < range.end && position < range.end && end > range.start {
            if groups.is_empty() {
                base = position;
            }
            groups.push(index);
            selected_rows = selected_rows
                .checked_add(rows)
                .ok_or_else(|| invalid("Parquet row count overflow"))?;
        }
        position = end;
    }
    if position != total {
        return Err(invalid("Parquet group row count mismatch"));
    }
    let usize_row = |row| usize::try_from(row).map_err(|_| invalid("row range width"));
    let selection = RowSelection::from_consecutive_ranges(
        std::iter::once(usize_row(range.start - base)?..usize_row(range.end - base)?),
        usize_row(selected_rows)?,
    );
    original.check()?;
    builder
        .with_row_groups(groups)
        .with_row_selection(selection)
        .with_batch_size(1024)
        .build()
        .map_err(io::Error::other)
}

fn column_kind(column: &parquet::schema::types::ColumnDescriptor) -> io::Result<(Kind, bool)> {
    if column.path().parts().len() != 1 || column.max_rep_level() != 0 || column.max_def_level() > 1
    {
        return Err(invalid("noncanonical Parquet column"));
    }
    let kind = match (column.physical_type(), column.logical_type_ref()) {
        (PhysicalType::BYTE_ARRAY, Some(LogicalType::String)) => Kind::String,
        (PhysicalType::INT64, None) => Kind::Int64,
        (PhysicalType::INT64, Some(value)) if *value == LogicalType::integer(64, false) => {
            Kind::UInt64
        }
        (PhysicalType::BOOLEAN, None) => Kind::Bool,
        _ => return Err(invalid("noncanonical Parquet column type")),
    };
    checked_name(column.name())?;
    Ok((kind, column.max_def_level() == 1))
}

fn group_range(metadata: &ParquetMetaData, offset: u64, rows: u64) -> io::Result<Range<usize>> {
    let total = u64::try_from(metadata.file_metadata().num_rows())
        .map_err(|_| invalid("negative Parquet row count"))?;
    let end = offset
        .checked_add(rows)
        .ok_or_else(|| invalid("row range overflow"))?;
    bounds(offset..end, total)?;
    let mut position = 0u64;
    let mut first = (offset == 0).then_some(0);
    let mut last = (end == 0).then_some(0);
    for (index, group) in metadata.row_groups().iter().enumerate() {
        position = position
            .checked_add(
                u64::try_from(group.num_rows())
                    .map_err(|_| invalid("negative Parquet group rows"))?,
            )
            .ok_or_else(|| invalid("Parquet row count overflow"))?;
        if position == offset {
            first = Some(index + 1);
        }
        if position == end {
            last = Some(index + 1);
        }
    }
    if position != total {
        return Err(invalid("Parquet group row count mismatch"));
    }
    match (first, last) {
        (Some(first), Some(last)) if first <= last => Ok(first..last),
        _ => Err(invalid("row slice must contain complete row groups")),
    }
}

struct CheckedChunk<'a> {
    file: &'a File,
    original: &'a SnapshotContext,
}
struct CheckedRead<'a> {
    inner: BufReader<File>,
    original: &'a SnapshotContext,
}
impl Read for CheckedRead<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.original.check()?;
        self.inner.read(buffer)
    }
}
impl Length for CheckedChunk<'_> {
    fn len(&self) -> u64 {
        self.file.len()
    }
}
impl<'a> ChunkReader for CheckedChunk<'a> {
    type T = CheckedRead<'a>;
    fn get_read(&self, start: u64) -> parquet::errors::Result<Self::T> {
        self.original.check()?;
        Ok(CheckedRead {
            inner: self.file.get_read(start)?,
            original: self.original,
        })
    }
    fn get_bytes(&self, start: u64, length: usize) -> parquet::errors::Result<Bytes> {
        self.original.check()?;
        self.file.get_bytes(start, length)
    }
}

fn footer(path: &Path, original: &SnapshotContext) -> io::Result<ParquetMetaData> {
    original.check()?;
    let file = File::open(path)?;
    let metadata = ParquetMetaDataReader::new()
        .parse_and_finish(&CheckedChunk {
            file: &file,
            original,
        })
        .map_err(|error| {
            original
                .check()
                .err()
                .unwrap_or_else(|| io::Error::other(error))
        })?;
    original.check()?;
    Ok(metadata)
}

/// Read the physical row count from the official Parquet footer, without reading pages.
pub fn parquet_rows(path: &Path, original: &SnapshotContext) -> io::Result<u64> {
    u64::try_from(footer(path, original)?.file_metadata().num_rows())
        .map_err(|_| invalid("negative Parquet row count"))
}

/// Concatenate complete row-group slices, preserving each input's physical boundary.
/// Returns `(row_offset, rows)` in input order. Only paths/schema metadata are
/// retained; existing encoded columns stream through the official Parquet writer.
/// Missing OPTIONAL columns become NULL; incompatible or missing REQUIRED columns fail.
/// The destination must not exist and is published only after successful finalization.
pub fn concat_parquet_rows(
    path: &Path,
    inputs: impl IntoIterator<Item = io::Result<(PathBuf, u64, u64)>>,
    original: &SnapshotContext,
) -> io::Result<Vec<(u64, u64)>> {
    original.check()?;
    let mut slices = Vec::new();
    let mut columns = BTreeMap::<String, (Kind, bool, usize)>::new();
    let mut metadata = BTreeMap::new();
    for input in inputs {
        original.check()?;
        let (file, offset, rows) = input?;
        let cached = match metadata.entry(file.clone()) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(footer(&file, original)?)
            }
        };
        group_range(cached, offset, rows)?;
        let schema = cached.file_metadata().schema_descr();
        let mut names = std::collections::BTreeSet::new();
        for column in schema.columns() {
            if !names.insert(column.name()) {
                return Err(invalid("duplicate Parquet column"));
            }
            let (kind, optional) = column_kind(column)?;
            let existing = columns
                .entry(column.name().to_owned())
                .or_insert((kind, optional, 0));
            if existing.0 != kind || existing.1 != optional {
                return Err(invalid("incompatible Parquet column schema"));
            }
            existing.2 += 1;
        }
        slices.push((file, offset, rows));
    }
    if slices.is_empty() || columns.is_empty() {
        return Err(invalid("missing Parquet inputs"));
    }
    if columns
        .values()
        .any(|(_, optional, count)| !optional && *count != slices.len())
    {
        return Err(invalid("missing required Parquet column"));
    }
    original.check()?;
    let temporary = tempfile::NamedTempFile::new_in(
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
    )?;
    let mut writer = file_writer(
        temporary.path(),
        columns
            .iter()
            .map(|(name, (kind, optional, _))| (name.clone(), *kind, *optional)),
    )?;
    let mut positions = Vec::with_capacity(slices.len());
    let mut next_row = 0u64;
    for (file, offset, rows) in slices {
        original.check()?;
        let cached = metadata
            .get(&file)
            .ok_or_else(|| invalid("missing Parquet input metadata"))?;
        let file = File::open(file)?;
        let groups = group_range(cached, offset, rows)?;
        for index in groups {
            original.check()?;
            let source = cached.row_group(index);
            let group_rows = u64::try_from(source.num_rows())
                .map_err(|_| invalid("negative Parquet group rows"))?;
            let mut group = writer.next_row_group().map_err(io::Error::other)?;
            for (name, (kind, optional, _)) in &columns {
                original.check()?;
                if let Some(column) = source
                    .columns()
                    .iter()
                    .find(|column| column.column_descr().name() == name)
                {
                    if column_kind(column.column_descr())? != (*kind, *optional) {
                        return Err(invalid("Parquet source schema changed"));
                    }
                    group
                        .append_column(
                            &CheckedChunk {
                                file: &file,
                                original,
                            },
                            ColumnCloseResult {
                                bytes_written: u64::try_from(column.compressed_size())
                                    .map_err(|_| invalid("negative Parquet column size"))?,
                                rows_written: group_rows,
                                metadata: column.clone(),
                                bloom_filter: None,
                                column_index: None,
                                offset_index: None,
                            },
                        )
                        .map_err(|error| {
                            original
                                .check()
                                .err()
                                .unwrap_or_else(|| io::Error::other(error))
                        })?;
                } else {
                    if !optional {
                        return Err(invalid("missing required Parquet column"));
                    }
                    let mut column = group
                        .next_column()
                        .map_err(io::Error::other)?
                        .ok_or_else(|| invalid("missing Parquet output column"))?;
                    let definitions = [0i16; 1024];
                    let mut remaining = group_rows;
                    while remaining != 0 {
                        original.check()?;
                        let count = remaining.min(1024) as usize;
                        let definitions = Some(&definitions[..count]);
                        match kind {
                            Kind::String => {
                                column
                                    .typed::<ByteArrayType>()
                                    .write_batch(&[], definitions, None)
                            }
                            Kind::Int64 | Kind::UInt64 => {
                                column
                                    .typed::<Int64Type>()
                                    .write_batch(&[], definitions, None)
                            }
                            Kind::Bool => {
                                column
                                    .typed::<BoolType>()
                                    .write_batch(&[], definitions, None)
                            }
                        }
                        .map_err(io::Error::other)?;
                        remaining -= count as u64;
                    }
                    column.close().map_err(io::Error::other)?;
                }
            }
            group.close().map_err(io::Error::other)?;
        }
        positions.push((next_row, rows));
        next_row = next_row
            .checked_add(rows)
            .filter(|rows| *rows <= i64::MAX as u64)
            .ok_or_else(|| invalid("Parquet row count overflow"))?;
    }
    writer.close().map_err(io::Error::other)?;
    temporary.as_file().sync_all()?;
    original.check()?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    Ok(positions)
}

#[cfg(test)]
mod tests;
