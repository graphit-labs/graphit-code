//! Validation and SDK decoding share immutable bytes owned by captured ranges.
use crate::{
    error::{DataReadError, Result, add, bounded},
    manifest::{Column, relative},
    policy::{DecodeLimits, LocalGraphLimits},
    protocol,
};
use arrow_array_parquet::{Array, BooleanArray, Int64Array, StringArray, UInt64Array};
use bytes::Bytes;
use graphit_core::artifact_store::{CapturedSourceFile, CapturedSourceReader, SnapshotContext};
use parquet::{
    arrow::{
        ProjectionMask,
        arrow_reader::{
            ArrowReaderMetadata, ArrowReaderOptions, ParquetRecordBatchReaderBuilder, RowSelection,
        },
    },
    errors::ParquetError,
    file::{
        metadata::{ParquetMetaData, ParquetMetaDataReader},
        reader::{ChunkReader, Length},
    },
    record::{Field, Row},
};
use polars_parquet_format::{
    self as format, CompressionCodec, ConvertedType, Encoding, FieldRepetitionType, LogicalType,
    PageType,
};
use std::{
    collections::BTreeMap,
    io::Cursor,
    ops::Range,
    sync::{
        Arc, Mutex, MutexGuard, TryLockError,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scalar {
    String,
    Int64,
    Bool,
    UInt64,
}
#[derive(Clone, Copy)]
pub(crate) enum LookupKey<'a> {
    String(&'a str),
    Int64(i64),
    UInt64(u64),
}
impl LookupKey<'_> {
    /// Modern min/max are conservative bounds even when truncated by the writer.
    /// Missing or legacy statistics cannot exclude a row group.
    pub(crate) fn may_match(
        self,
        statistics: Option<&parquet::file::statistics::Statistics>,
    ) -> Result<bool> {
        use parquet::file::statistics::Statistics;
        let Some(stats) = statistics.filter(|s| !s.is_min_max_deprecated()) else {
            return Ok(true);
        };
        match (self, stats) {
            (Self::String(key), Statistics::ByteArray(stats)) => {
                if stats
                    .min_opt()
                    .zip(stats.max_opt())
                    .is_some_and(|(min, max)| min.data() > max.data())
                {
                    return Err(invalid("statistics bounds"));
                }
                Ok(stats.min_opt().is_none_or(|v| key.as_bytes() >= v.data())
                    && stats.max_opt().is_none_or(|v| key.as_bytes() <= v.data()))
            }
            (Self::Int64(key), Statistics::Int64(stats)) => {
                if stats
                    .min_opt()
                    .zip(stats.max_opt())
                    .is_some_and(|(min, max)| min > max)
                {
                    return Err(invalid("statistics bounds"));
                }
                Ok(stats.min_opt().is_none_or(|v| key >= *v)
                    && stats.max_opt().is_none_or(|v| key <= *v))
            }
            (Self::UInt64(key), Statistics::Int64(stats)) => {
                if stats
                    .min_opt()
                    .zip(stats.max_opt())
                    .is_some_and(|(min, max)| (*min as u64) > (*max as u64))
                {
                    return Err(invalid("statistics bounds"));
                }
                Ok(stats.min_opt().is_none_or(|v| key >= *v as u64)
                    && stats.max_opt().is_none_or(|v| key <= *v as u64))
            }
            _ => Ok(true),
        }
    }
    fn matches(self, value: &Field, strings: usize) -> Result<bool> {
        match (self, value) {
            (Self::String(key), Field::Str(value)) => {
                bounded(value.len(), strings, "primary key")?;
                Ok(key == value)
            }
            (Self::Int64(key), Field::Long(value)) => Ok(key == *value),
            (Self::UInt64(key), Field::ULong(value)) => Ok(key == *value),
            _ => Err(invalid("nonnull lookup key")),
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Shape<'a> {
    Nodes(&'a [Column]),
    Indptr,
    Indices,
}
#[derive(Clone)]
pub(crate) struct Chunk {
    pub(crate) group: usize,
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) values: u64,
    pub(crate) name: String,
}
#[derive(Clone)]
pub(crate) struct Table {
    shards: Vec<(u64, Box<Table>)>,
    path: Arc<str>,
    file: Arc<CapturedSourceFile>,
    length: u64,
    footer_start: u64,
    footer: Bytes,
    trailer: Bytes,
    metadata: Arc<format::FileMetaData>,
    arrow: ArrowReaderMetadata,
    chunks: Arc<Vec<Chunk>>,
    row_groups: Vec<usize>,
    row_offset: u64,
    pub(crate) fields: Vec<(String, Scalar)>,
    pub(crate) rows: u64,
}
fn invalid(field: &'static str) -> DataReadError {
    DataReadError::InvalidParquet(field)
}
fn unsigned(value: i64, field: &'static str) -> Result<u64> {
    u64::try_from(value).map_err(|_| invalid(field))
}
fn scalar(schema: &format::SchemaElement) -> Result<Scalar> {
    if schema.num_children.is_some_and(|n| n != 0)
        || schema.type_length.is_some()
        || schema.scale.is_some()
        || schema.precision.is_some()
        || !matches!(
            schema.repetition_type,
            Some(FieldRepetitionType::REQUIRED | FieldRepetitionType::OPTIONAL)
        )
    {
        return Err(DataReadError::UnsupportedFormat("flat scalar schema"));
    }
    let scalar = match schema.type_ {
        Some(format::Type::BYTE_ARRAY)
            if matches!(schema.converted_type, None | Some(ConvertedType::UTF8))
                && matches!(schema.logical_type, None | Some(LogicalType::STRING(_)))
                && (schema.converted_type.is_some() || schema.logical_type.is_some()) =>
        {
            Scalar::String
        }
        Some(format::Type::BOOLEAN)
            if schema.converted_type.is_none() && schema.logical_type.is_none() =>
        {
            Scalar::Bool
        }
        Some(format::Type::INT64) => {
            let integer = match &schema.logical_type {
                Some(LogicalType::INTEGER(integer)) if integer.bit_width == 64 => {
                    Some(integer.is_signed)
                }
                None => None,
                _ => return Err(DataReadError::UnsupportedFormat("integer annotation")),
            };
            match (schema.converted_type, integer) {
                (Some(ConvertedType::UINT_64), None | Some(false)) | (None, Some(false)) => {
                    Scalar::UInt64
                }
                (None | Some(ConvertedType::INT_64), None | Some(true)) => Scalar::Int64,
                _ => return Err(DataReadError::UnsupportedFormat("integer annotation")),
            }
        }
        _ => return Err(DataReadError::UnsupportedFormat("scalar annotation")),
    };
    Ok(scalar)
}
#[derive(Clone, Copy)]
pub(crate) struct DecodeRequest<'a> {
    pub(crate) source: &'a CapturedSourceReader,
    pub(crate) context: &'a Arc<SnapshotContext>,

    pub(crate) limits: LocalGraphLimits,
    pub(crate) decode: DecodeLimits,
    pub(crate) work: &'a Arc<Work>,
}
impl Table {
    pub(crate) fn capture_nodes(
        reader: &CapturedSourceReader,
        node: &crate::manifest::NodeTable,
        limits: LocalGraphLimits,
        captured: &mut BTreeMap<String, Self>,
    ) -> Result<Self> {
        let mut shards = Vec::new();
        for shard in &node.shards {
            let part = Self::capture(
                reader,
                &shard.file,
                shard.row_offset,
                shard.rows,
                Shape::Nodes(&node.columns),
                limits,
                captured,
            )?;
            if shard.rows != 0
                && !part
                    .fields
                    .iter()
                    .any(|(name, _)| name == &node.primary_key)
            {
                return Err(invalid("shard primary key"));
            }
            shards.push((shard.row_base, Box::new(part)));
        }
        let mut table = shards
            .first()
            .ok_or_else(|| invalid("empty node shards"))?
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
                        _ => return Err(invalid("manifest type")),
                    },
                ))
            })
            .collect::<Result<_>>()?;
        table.rows = node.rows;
        table.shards = shards;
        Ok(table)
    }
    pub(crate) fn seed_rows(&self, count: u64) -> Result<Vec<u64>> {
        if self.shards.is_empty() {
            return Ok((0..self.rows.min(count)).collect());
        }
        let mut selected = Vec::new();
        let count = usize::try_from(count).map_err(|_| invalid("seed count"))?;
        for (base, part) in &self.shards {
            let end = base
                .checked_add(part.rows)
                .ok_or_else(|| invalid("node row extent"))?;
            selected.extend((*base..end).take(count.saturating_sub(selected.len())));
            if selected.len() == count {
                break;
            }
        }
        Ok(selected)
    }
    pub(crate) fn contains(&self, offset: u64) -> bool {
        if self.shards.is_empty() {
            return offset < self.rows;
        }
        self.shards
            .iter()
            .any(|(base, part)| offset >= *base && offset - *base < part.rows)
    }
    pub(crate) fn relative_path(&self) -> &str {
        &self.path
    }
    pub(crate) fn row_offset(&self) -> u64 {
        self.row_offset
    }
    pub(crate) fn capture(
        reader: &CapturedSourceReader,
        path: &str,
        row_offset: u64,
        rows: u64,
        shape: Shape<'_>,
        limits: LocalGraphLimits,
        captured: &mut BTreeMap<String, Self>,
    ) -> Result<Self> {
        if let Some(physical) = captured.get(path) {
            validate_shape(&physical.fields, shape)?;
            let mut table = physical.clone();
            table.row_groups = slice_row_groups(
                physical.metadata.num_rows,
                physical
                    .metadata
                    .row_groups
                    .iter()
                    .map(|group| group.num_rows),
                row_offset,
                rows,
                shape,
            )?;
            table.row_offset = row_offset;
            table.rows = rows;
            return Ok(table);
        }
        let file = reader.open_file(relative(path)?)?;
        let length = file.len();
        if length < 12 || file.read_range(0, 4)?.as_ref() != b"PAR1" {
            return Err(invalid("magic"));
        }

        let trailer = file.read_range(length - 8, 8)?;
        if trailer.as_ref().get(4..) != Some(b"PAR1".as_slice()) {
            return Err(invalid("trailer magic"));
        }
        let mut size = [0; 4];
        size.copy_from_slice(&trailer.as_ref()[..4]);
        let footer_len = u32::from_le_bytes(size) as usize;
        bounded(footer_len, limits.max_footer_bytes, "footer bytes")?;
        let footer_start = length
            .checked_sub(8)
            .and_then(|n| n.checked_sub(footer_len as u64))
            .filter(|n| *n >= 4)
            .ok_or_else(|| invalid("footer span"))?;

        let bytes = file.read_range(footer_start, footer_len)?;
        let metadata = protocol::footer(bytes.as_ref(), limits)?;
        let footer = Bytes::from_owner(bytes);
        let trailer = Bytes::from_owner(trailer);
        let ValidatedFooter { fields, chunks } = validate(
            &metadata,
            unsigned(metadata.num_rows, "rows")?,
            shape,
            footer_start,
            limits,
        )?;
        let row_groups = slice_row_groups(
            metadata.num_rows,
            metadata.row_groups.iter().map(|group| group.num_rows),
            row_offset,
            rows,
            shape,
        )?;
        let decoded = Arc::new(
            ParquetMetaDataReader::decode_metadata(&footer).map_err(|_| invalid("SDK footer"))?,
        );
        compare(&metadata, &decoded)?;
        let arrow = ArrowReaderMetadata::try_new(
            decoded,
            ArrowReaderOptions::new().with_skip_arrow_metadata(true),
        )
        .map_err(|_| invalid("SDK schema"))?;

        let table = Self {
            shards: Vec::new(),
            path: path.into(),
            file: Arc::new(file),
            length,
            footer_start,
            footer,
            trailer,
            metadata: Arc::new(metadata),
            arrow,
            chunks: Arc::new(chunks),
            row_groups,
            row_offset,
            fields,
            rows,
        };
        let mut physical = table.clone();
        physical.rows = unsigned(physical.metadata.num_rows, "rows")?;
        physical.row_offset = 0;
        physical.row_groups = (0..physical.metadata.row_groups.len()).collect();
        captured.insert(path.into(), physical);
        Ok(table)
    }
    pub(crate) fn scan(
        &self,
        request: &DecodeRequest<'_>,
        names: &[&str],
        mut consume: impl FnMut(u64, &Row) -> Result<()>,
    ) -> Result<()> {
        self.scan_inner(request, names, &mut consume)
    }
    /// Resolve exact keys through captured SDK statistics and projected bodies.
    /// Logical ordinals remain relative to this table's captured shard slices.
    pub(crate) fn scan_keys(
        &self,
        request: &DecodeRequest<'_>,
        name: &str,
        keys: &[LookupKey<'_>],
        strings: usize,
        names: &[&str],
        mut consume: impl FnMut(u64, &Row) -> Result<()>,
    ) -> Result<()> {
        self.scan_keys_inner(request, name, keys, strings, names, &mut consume)
    }
    fn scan_keys_inner(
        &self,
        request: &DecodeRequest<'_>,
        name: &str,
        keys: &[LookupKey<'_>],
        strings: usize,
        names: &[&str],
        consume: &mut dyn FnMut(u64, &Row) -> Result<()>,
    ) -> Result<()> {
        request.context.check()?;
        request.source.check_request(request.context)?;
        if keys.is_empty() || self.rows == 0 {
            return Ok(());
        }
        if !self.shards.is_empty() {
            for (base, part) in &self.shards {
                part.scan_keys_inner(request, name, keys, strings, names, &mut |row, record| {
                    consume(
                        base.checked_add(row)
                            .ok_or_else(|| invalid("node row ordinal"))?,
                        record,
                    )
                })?;
            }
            return Ok(());
        }
        let column = self
            .fields
            .iter()
            .position(|(field, _)| field == name)
            .ok_or_else(|| invalid("lookup column"))?;
        let mut groups = Vec::new();
        for index in &self.row_groups {
            let group = self.arrow.metadata().row_group(*index);
            let mut matched = false;
            for key in keys {
                matched |= key.may_match(group.column(column).statistics())?;
            }
            if matched {
                groups.push(*index);
            }
        }
        self.scan_groups(request, names, &groups, &mut |row, record| {
            let value = crate::field(record, name)?;
            let mut matched = false;
            for key in keys {
                matched |= key.matches(value, strings)?;
            }
            if matched {
                consume(row, record)?;
            }
            Ok(())
        })
    }
    /// Read an adjacency interval without allocating one ordinal per edge.
    pub(crate) fn scan_range(
        &self,
        request: &DecodeRequest<'_>,
        names: &[&str],
        range: Range<u64>,
        mut consume: impl FnMut(u64, &Row) -> Result<()>,
    ) -> Result<()> {
        request.context.check()?;
        request.source.check_request(request.context)?;
        if !self.shards.is_empty() || range.start > range.end || range.end > self.rows {
            return Err(invalid("selected range"));
        }
        if range.is_empty() {
            return Ok(());
        }
        let mut groups = Vec::new();
        let mut offset = 0u64;
        for (index, group) in self.arrow.metadata().row_groups().iter().enumerate() {
            let end = offset
                .checked_add(unsigned(group.num_rows(), "group rows")?)
                .ok_or_else(|| invalid("group rows"))?;
            if self.row_groups.binary_search(&index).is_ok()
                && offset < self.row_offset + range.end
                && end > self.row_offset + range.start
            {
                groups.push(index);
            }
            offset = end;
        }
        self.scan_groups(request, names, &groups, &mut |row, record| {
            if range.contains(&row) {
                consume(row, record)?;
            }
            Ok(())
        })
    }
    fn scan_groups(
        &self,
        request: &DecodeRequest<'_>,
        names: &[&str],
        groups: &[usize],
        consume: &mut dyn FnMut(u64, &Row) -> Result<()>,
    ) -> Result<()> {
        let mut physical = self.clone();
        physical.row_groups.clear();
        let mut segments = Vec::new();
        let mut offset = 0u64;
        let mut decoded = 0u64;
        for (index, group) in self.arrow.metadata().row_groups().iter().enumerate() {
            let count = unsigned(group.num_rows(), "group rows")?;
            if groups.binary_search(&index).is_ok() && count != 0 {
                let logical = offset
                    .checked_sub(self.row_offset)
                    .ok_or_else(|| invalid("row slice boundary"))?;
                segments.push((decoded, logical, count));
                decoded = decoded
                    .checked_add(count)
                    .ok_or_else(|| invalid("decoded rows"))?;
                physical.row_groups.push(index);
            }
            offset = offset
                .checked_add(count)
                .ok_or_else(|| invalid("group rows"))?;
        }
        physical.rows = decoded;
        let mut segment = 0;
        physical.scan(request, names, |ordinal, record| {
            while segments
                .get(segment)
                .is_some_and(|(base, _, count)| ordinal - base >= *count)
            {
                segment += 1;
            }
            let (base, logical, _) = segments
                .get(segment)
                .ok_or_else(|| invalid("decoded row count"))?;
            consume(
                logical
                    .checked_add(ordinal - base)
                    .ok_or_else(|| invalid("node row ordinal"))?,
                record,
            )
        })
    }
    pub(crate) fn row_group_ranges(&self) -> Result<Vec<Range<u64>>> {
        let mut ranges = Vec::new();
        if !self.shards.is_empty() {
            for (base, part) in &self.shards {
                for range in part.row_group_ranges()? {
                    ranges.push(
                        base.checked_add(range.start)
                            .ok_or_else(|| invalid("node row extent"))?
                            ..base
                                .checked_add(range.end)
                                .ok_or_else(|| invalid("node row extent"))?,
                    );
                }
            }
            return Ok(ranges);
        }
        let mut offset = 0u64;
        for (index, group) in self.metadata.row_groups.iter().enumerate() {
            let next = offset
                .checked_add(unsigned(group.num_rows, "group rows")?)
                .ok_or_else(|| invalid("group rows"))?;
            if self.row_groups.binary_search(&index).is_ok() && next > offset {
                ranges.push(
                    offset
                        .checked_sub(self.row_offset)
                        .ok_or_else(|| invalid("row slice boundary"))?
                        ..next
                            .checked_sub(self.row_offset)
                            .ok_or_else(|| invalid("row slice boundary"))?,
                );
            }
            offset = next;
        }
        Ok(ranges)
    }
    /// Decode complete intersecting SDK row groups, but deliver only selected
    /// logical rows. Skipped groups and shards never ask for body ranges.
    pub(crate) fn scan_selected(
        &self,
        request: &DecodeRequest<'_>,
        names: &[&str],
        rows: &[u64],
        mut consume: impl FnMut(u64, &Row) -> Result<()>,
    ) -> Result<()> {
        self.scan_selected_inner(request, names, rows, &mut consume)
    }
    fn scan_selected_inner(
        &self,
        request: &DecodeRequest<'_>,
        names: &[&str],
        rows: &[u64],
        consume: &mut dyn FnMut(u64, &Row) -> Result<()>,
    ) -> Result<()> {
        request.context.check()?;
        request.source.check_request(request.context)?;
        if rows.windows(2).any(|pair| pair[0] >= pair[1])
            || rows.iter().any(|row| !self.contains(*row))
        {
            return Err(invalid("selected rows"));
        }
        if rows.is_empty() {
            return Ok(());
        }
        if !self.shards.is_empty() {
            for (base, part) in &self.shards {
                let selected = rows
                    .iter()
                    .copied()
                    .filter(|row| *row >= *base && *row - *base < part.rows)
                    .map(|row| row - *base)
                    .collect::<Vec<_>>();
                if selected.is_empty() {
                    continue;
                }
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
                part.scan_selected_inner(request, &physical, &selected, &mut |row, record| {
                    let row = base
                        .checked_add(row)
                        .ok_or_else(|| invalid("node row ordinal"))?;
                    if physical.as_slice() == names {
                        consume(row, record)
                    } else {
                        consume(row, &project(names, record))
                    }
                })?;
            }
            return request.context.check().map_err(Into::into);
        }
        let mut physical = self.clone();
        physical.row_groups.clear();
        let mut segments = Vec::new();
        let mut offset = 0u64;
        let mut decoded = 0u64;
        for (index, group) in self.metadata.row_groups.iter().enumerate() {
            let count = unsigned(group.num_rows, "group rows")?;
            let next = offset
                .checked_add(count)
                .ok_or_else(|| invalid("group rows"))?;
            if self.row_groups.binary_search(&index).is_ok() && count != 0 {
                let start = offset
                    .checked_sub(self.row_offset)
                    .ok_or_else(|| invalid("row slice boundary"))?;
                let end = start
                    .checked_add(count)
                    .ok_or_else(|| invalid("row slice boundary"))?;
                let first = rows.partition_point(|row| *row < start);
                if rows.get(first).is_some_and(|row| *row < end) {
                    segments.push((decoded, start, count));
                    decoded = decoded
                        .checked_add(count)
                        .ok_or_else(|| invalid("decoded rows"))?;
                    physical.row_groups.push(index);
                }
            }
            offset = next;
        }
        physical.rows = decoded;
        let mut segment = 0usize;
        let mut delivered = 0usize;
        physical.scan(request, names, |ordinal, record| {
            while segments.get(segment).is_some_and(|(base, _, count)| {
                ordinal.checked_sub(*base).is_some_and(|row| row >= *count)
            }) {
                segment += 1;
            }
            let (base, logical, _) = segments
                .get(segment)
                .ok_or_else(|| invalid("decoded row count"))?;
            let row = logical
                .checked_add(
                    ordinal
                        .checked_sub(*base)
                        .ok_or_else(|| invalid("decoded row ordinal"))?,
                )
                .ok_or_else(|| invalid("node row ordinal"))?;
            if rows.binary_search(&row).is_ok() {
                consume(row, record)?;
                delivered += 1;
            }
            Ok(())
        })?;
        if delivered != rows.len() {
            return Err(invalid("selected row count"));
        }
        request.context.check().map_err(Into::into)
    }
    fn scan_inner(
        &self,
        request: &DecodeRequest<'_>,
        names: &[&str],
        consume: &mut dyn FnMut(u64, &Row) -> Result<()>,
    ) -> Result<()> {
        if !self.shards.is_empty() {
            for (base, part) in &self.shards {
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
                if physical.as_slice() == names {
                    part.scan(request, names, |row, record| {
                        consume(
                            base.checked_add(row)
                                .ok_or_else(|| invalid("node row ordinal"))?,
                            record,
                        )
                    })?;
                    continue;
                }
                part.scan(request, &physical, |row, record| {
                    let projected = project(names, record);
                    consume(
                        base.checked_add(row)
                            .ok_or_else(|| invalid("node row ordinal"))?,
                        &projected,
                    )
                })?;
            }
            return Ok(());
        }
        let DecodeRequest {
            source,
            context,

            limits,
            decode,
            work,
        } = *request;
        context.check()?;
        source.check_request(context)?;
        if self.rows == 0 {
            return Ok(());
        }
        let adapter = Adapter::new(Arc::clone(&self.file), self, context, limits, work)?;
        work.page()?;
        let builder =
            ParquetRecordBatchReaderBuilder::new_with_metadata(adapter.clone(), self.arrow.clone());
        compare(&self.metadata, builder.metadata())?;
        let selected = self
            .fields
            .iter()
            .enumerate()
            .filter(|(_, (name, _))| names.contains(&name.as_str()))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if selected.len() != names.len() {
            return Err(invalid("projection"));
        }
        let projection = ProjectionMask::roots(builder.parquet_schema(), selected);
        let count = usize::try_from(self.rows).map_err(|_| invalid("table row count"))?;
        work.rows(count)?;
        let selected = RowSelection::from_consecutive_ranges(std::iter::once(0..count), count);
        let sdk = builder
            .with_row_groups(self.row_groups.clone())
            .with_projection(projection)
            .with_row_selection(selected)
            .with_batch_size(decode.row_batch_size)
            .build()
            .map_err(|_| adapter.failure("row decoder"))?;
        let mut ordinal = 0;
        for batch in sdk {
            context.check()?;
            let batch = batch.map_err(|_| adapter.failure("row decoder"))?;
            let arrays = names
                .iter()
                .map(|name| {
                    batch
                        .column_by_name(name)
                        .ok_or_else(|| invalid("decoded projection"))
                })
                .collect::<Result<Vec<_>>>()?;
            for at in 0..batch.num_rows() {
                context.check()?;
                let mut values = Vec::with_capacity(batch.num_columns());
                for (name, array) in names.iter().zip(&arrays) {
                    let value = if array.is_null(at) {
                        Field::Null
                    } else if let Some(value) = array.as_any().downcast_ref::<StringArray>() {
                        Field::Str(value.value(at).to_owned())
                    } else if let Some(value) = array.as_any().downcast_ref::<Int64Array>() {
                        Field::Long(value.value(at))
                    } else if let Some(value) = array.as_any().downcast_ref::<UInt64Array>() {
                        Field::ULong(value.value(at))
                    } else if let Some(value) = array.as_any().downcast_ref::<BooleanArray>() {
                        Field::Bool(value.value(at))
                    } else {
                        return Err(invalid("decoded scalar"));
                    };
                    values.push(((*name).to_owned(), value));
                }
                let row = Row::new(values);
                consume(ordinal, &row)?;
                ordinal = ordinal
                    .checked_add(1)
                    .ok_or_else(|| invalid("decoded row count"))?;
            }
        }
        if ordinal != self.rows {
            return Err(invalid("table row count"));
        }
        for &group in &self.row_groups {
            adapter.complete_group(group, names)?;
        }
        context.check()?;
        Ok(())
    }
}
fn project(names: &[&str], record: &Row) -> Row {
    Row::new(
        names
            .iter()
            .map(|name| {
                (
                    (*name).to_owned(),
                    record
                        .get_column_iter()
                        .find(|(field, _)| field.as_str() == *name)
                        .map(|(_, value)| value.clone())
                        .unwrap_or(Field::Null),
                )
            })
            .collect(),
    )
}
pub(crate) struct ValidatedFooter {
    pub(crate) fields: Vec<(String, Scalar)>,
    pub(crate) chunks: Vec<Chunk>,
}
pub(crate) fn slice_row_groups(
    physical_rows: i64,
    groups: impl IntoIterator<Item = i64>,
    row_offset: u64,
    rows: u64,
    shape: Shape<'_>,
) -> Result<Vec<usize>> {
    let physical_rows = unsigned(physical_rows, "rows")?;
    let end = row_offset
        .checked_add(rows)
        .filter(|end| *end <= physical_rows)
        .ok_or_else(|| invalid("row slice bounds"))?;
    let mut offset = 0u64;
    let mut start_boundary = row_offset == 0;
    let mut end_boundary = end == 0;
    let mut selected = Vec::new();
    for (index, group_rows) in groups.into_iter().enumerate() {
        let next = offset
            .checked_add(unsigned(group_rows, "group rows")?)
            .filter(|next| *next <= physical_rows)
            .ok_or_else(|| invalid("group rows"))?;
        if offset < end && next > row_offset && next > offset {
            if offset < row_offset || next > end {
                return Err(invalid("row slice boundary"));
            }
            selected.push(index);
        }
        start_boundary |= next == row_offset;
        end_boundary |= next == end;
        offset = next;
    }
    if offset != physical_rows || !start_boundary || !end_boundary {
        return Err(invalid("row slice boundary"));
    }
    if matches!(shape, Shape::Indptr) && selected.len() != 1 {
        return Err(invalid("indptr row groups"));
    }
    Ok(selected)
}
pub(crate) fn validate_shape(fields: &[(String, Scalar)], shape: Shape<'_>) -> Result<()> {
    match shape {
        Shape::Nodes(columns) => {
            for (name, kind) in fields {
                let column = columns
                    .iter()
                    .find(|column| column.name == *name)
                    .ok_or_else(|| invalid("manifest columns"))?;
                let expected = match column.kind.as_str() {
                    "STRING" => Scalar::String,
                    "INT64" => Scalar::Int64,
                    "BOOL" => Scalar::Bool,
                    _ => return Err(invalid("manifest type")),
                };
                if expected != *kind {
                    return Err(invalid("manifest schema"));
                }
            }
        }
        Shape::Indptr if fields.len() == 1 && fields[0] == ("ptr".into(), Scalar::UInt64) => (),
        Shape::Indices
            if fields.first() == Some(&("target".into(), Scalar::UInt64))
                && fields
                    .iter()
                    .any(|(name, kind)| name == "uid" && *kind == Scalar::String)
                && fields
                    .iter()
                    .skip(1)
                    .all(|(_, kind)| *kind != Scalar::UInt64) => {}
        _ => return Err(invalid("CSR schema")),
    }
    Ok(())
}
pub(crate) fn validate(
    metadata: &format::FileMetaData,
    rows: u64,
    shape: Shape<'_>,
    footer_start: u64,
    limits: LocalGraphLimits,
) -> Result<ValidatedFooter> {
    if metadata.version != 1
        || unsigned(metadata.num_rows, "rows")? != rows
        || metadata.encryption_algorithm.is_some()
        || metadata.footer_signing_key_metadata.is_some()
    {
        return Err(invalid("footer identity"));
    }
    let root = metadata
        .schema
        .first()
        .ok_or_else(|| invalid("schema root"))?;
    bounded(root.name.len(), limits.max_name_bytes, "root name")?;
    if root.logical_type.is_some()
        || root.converted_type.is_some()
        || root.type_length.is_some()
        || root.scale.is_some()
        || root.precision.is_some()
        || root.type_.is_some()
        || root.repetition_type.is_some()
        || root.num_children != Some((metadata.schema.len() - 1) as i32)
    {
        return Err(invalid("schema root"));
    }
    bounded(metadata.schema.len() - 1, limits.max_columns, "columns")?;
    bounded(
        metadata.row_groups.len(),
        limits.max_row_groups,
        "row groups",
    )?;
    if rows != 0 && metadata.schema.len() == 1 {
        return Err(invalid("empty nonzero schema"));
    }
    let mut fields = Vec::new();
    for schema in metadata.schema.iter().skip(1) {
        bounded(schema.name.len(), limits.max_name_bytes, "column name")?;
        if schema.name.is_empty() || fields.iter().any(|(name, _)| name == &schema.name) {
            return Err(invalid("column identity"));
        }
        let kind = scalar(schema)?;
        fields
            .try_reserve(1)
            .map_err(|_| DataReadError::LimitExceeded("schema allocation"))?;
        fields.push((schema.name.clone(), kind));
    }
    validate_shape(&fields, shape)?;
    let mut metadata_count = 0usize;
    let mut metadata_text = metadata.created_by.as_ref().map_or(0, String::len);
    let mut marker = false;
    let mut keys = std::collections::BTreeSet::new();
    for item in metadata.key_value_metadata.iter().flatten() {
        metadata_count = add(metadata_count, 1, "metadata entries")?;
        metadata_text = add(metadata_text, item.key.len(), "metadata bytes")?;
        metadata_text = add(
            metadata_text,
            item.value.as_ref().map_or(0, String::len),
            "metadata bytes",
        )?;
        if !keys.insert(&item.key) {
            return Err(invalid("duplicate metadata key"));
        }
        if item.key == "icebug_disk_version" {
            marker = item.value.as_deref() == Some("v1");
        }
    }
    if !marker {
        return Err(invalid("disk version"));
    }
    bounded(
        metadata_count,
        limits.max_metadata_entries,
        "metadata entries",
    )?;
    bounded(metadata_text, limits.max_metadata_bytes, "metadata bytes")?;
    let mut chunks = Vec::new();
    let mut spans = Vec::new();
    let mut group_rows = 0u64;
    for (group_index, group) in metadata.row_groups.iter().enumerate() {
        let count = unsigned(group.num_rows, "group rows")?;
        group_rows = group_rows
            .checked_add(count)
            .ok_or_else(|| invalid("group rows"))?;
        if group.columns.len() != fields.len() {
            return Err(invalid("group columns"));
        }
        for (index, column) in group.columns.iter().enumerate() {
            if column.file_path.is_some()
                || column.crypto_metadata.is_some()
                || column.encrypted_column_metadata.is_some()
            {
                return Err(invalid("external/encrypted column"));
            }
            let data = column
                .meta_data
                .as_ref()
                .ok_or_else(|| invalid("column metadata"))?;
            let mut column_keys = std::collections::BTreeSet::new();
            for item in data.key_value_metadata.iter().flatten() {
                metadata_count = add(metadata_count, 1, "metadata entries")?;
                metadata_text = add(metadata_text, item.key.len(), "metadata bytes")?;
                metadata_text = add(
                    metadata_text,
                    item.value.as_ref().map_or(0, String::len),
                    "metadata bytes",
                )?;
                bounded(
                    metadata_count,
                    limits.max_metadata_entries,
                    "metadata entries",
                )?;
                bounded(metadata_text, limits.max_metadata_bytes, "metadata bytes")?;
                if !column_keys.insert(&item.key) {
                    return Err(invalid("duplicate column metadata key"));
                }
            }
            if data.path_in_schema.len() != 1
                || data.path_in_schema[0] != fields[index].0
                || Some(data.type_) != metadata.schema[index + 1].type_
                || unsigned(data.num_values, "column values")? != count
            {
                return Err(invalid("column identity/count"));
            }
            if data.codec != CompressionCodec::ZSTD
                || data.dictionary_page_offset.is_some()
                || data.encodings.iter().any(|encoding| {
                    !matches!(
                        *encoding,
                        Encoding::PLAIN | Encoding::RLE | Encoding::BIT_PACKED
                    )
                })
            {
                return Err(DataReadError::UnsupportedFormat("codec/encoding"));
            }
            let start = unsigned(data.data_page_offset, "page offset")?;
            let bytes = unsigned(data.total_compressed_size, "chunk size")?;
            unsigned(data.total_uncompressed_size, "uncompressed chunk size")?;
            if bytes > limits.max_chunk_bytes as u64 {
                return Err(DataReadError::LimitExceeded("chunk bytes"));
            }
            let end = start
                .checked_add(bytes)
                .filter(|end| start >= 4 && *end <= footer_start)
                .ok_or_else(|| invalid("chunk span"))?;
            if count > 0 && bytes == 0 {
                return Err(invalid("empty chunk"));
            }
            if bytes > 0 {
                spans
                    .try_reserve(1)
                    .map_err(|_| DataReadError::LimitExceeded("chunk metadata"))?;
                spans.push((start, end));
            }
            chunks
                .try_reserve(1)
                .map_err(|_| DataReadError::LimitExceeded("chunk metadata"))?;
            chunks.push(Chunk {
                group: group_index,
                start,
                end,
                values: count,
                name: fields[index].0.clone(),
            });
        }
    }
    if group_rows != rows {
        return Err(invalid("group row total"));
    }
    // Empty ranges occupy no bytes. Sort only the validation spans so physical
    // chunk order stays intact; any overlap occurs between sorted neighbors.
    spans.sort_unstable();
    if spans.windows(2).any(|pair| pair[1].0 < pair[0].1) {
        return Err(invalid("overlapping chunks"));
    }
    Ok(ValidatedFooter { fields, chunks })
}
fn compare(metadata: &format::FileMetaData, sdk: &ParquetMetaData) -> Result<()> {
    let file = sdk.file_metadata();
    if file.num_rows() != metadata.num_rows
        || sdk.num_row_groups() != metadata.row_groups.len()
        || file.schema_descr().num_columns() + 1 != metadata.schema.len()
    {
        return Err(invalid("parser differential"));
    }
    for (index, field) in metadata.schema.iter().skip(1).enumerate() {
        let actual = file.schema_descr().column(index);
        if actual.name() != field.name
            || actual.physical_type() as i32 != field.type_.ok_or_else(|| invalid("schema type"))?.0
            || actual.max_rep_level() != 0
            || actual.max_def_level()
                != i16::from(field.repetition_type == Some(FieldRepetitionType::OPTIONAL))
        {
            return Err(invalid("schema differential"));
        }
    }
    for (group, admitted) in sdk.row_groups().iter().zip(&metadata.row_groups) {
        if group.num_rows() != admitted.num_rows || group.num_columns() != admitted.columns.len() {
            return Err(invalid("group differential"));
        }
        for (actual, admitted) in group.columns().iter().zip(&admitted.columns) {
            let admitted = admitted
                .meta_data
                .as_ref()
                .ok_or_else(|| invalid("metadata"))?;
            if actual.num_values() != admitted.num_values
                || actual.data_page_offset() != admitted.data_page_offset
                || actual.compressed_size() != admitted.total_compressed_size
                || actual.uncompressed_size() != admitted.total_uncompressed_size
                || actual.dictionary_page_offset() != admitted.dictionary_page_offset
                || actual.compression() != parquet::basic::Compression::ZSTD(Default::default())
            {
                return Err(invalid("chunk differential"));
            }
        }
    }
    Ok(())
}

pub(crate) struct Work {
    rows: AtomicUsize,
    pages: AtomicUsize,
    max_rows: usize,
    max_pages: usize,
}
impl Work {
    pub(crate) fn new(limits: DecodeLimits) -> Arc<Self> {
        Arc::new(Self {
            rows: AtomicUsize::new(0),
            pages: AtomicUsize::new(0),
            max_rows: limits.max_decoded_rows,
            max_pages: limits.max_page_work,
        })
    }
    #[cfg(test)]
    pub(crate) fn counts(&self) -> (usize, usize) {
        (
            self.rows.load(Ordering::Acquire),
            self.pages.load(Ordering::Acquire),
        )
    }
    pub(crate) fn rows(&self, amount: usize) -> Result<()> {
        self.rows
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                n.checked_add(amount).filter(|n| *n <= self.max_rows)
            })
            .map(|_| ())
            .map_err(|_| DataReadError::LimitExceeded("decoded rows"))
    }
    pub(crate) fn page(&self) -> Result<()> {
        self.pages
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                n.checked_add(1).filter(|n| *n <= self.max_pages)
            })
            .map(|_| ())
            .map_err(|_| DataReadError::LimitExceeded("page work"))
    }
}
struct Frame {
    start: u64,
    body: u64,
    end: u64,
    bytes: Bytes,
    chunk: usize,
}
struct ChunkProgress {
    next: u64,
    values: u64,
    pages: usize,
}
struct Cache {
    frames: Vec<Frame>,
    progress: Vec<ChunkProgress>,
    error: Option<DataReadError>,
}
struct Inner {
    file: Arc<CapturedSourceFile>,
    length: u64,
    footer_start: u64,
    footer: Bytes,
    trailer: Bytes,
    chunks: Vec<Chunk>,
    context: Arc<SnapshotContext>,

    limits: LocalGraphLimits,
    work: Arc<Work>,
    cache: Mutex<Cache>,
}
#[derive(Clone)]
struct Adapter(Arc<Inner>);
impl Adapter {
    fn new(
        file: impl Into<Arc<CapturedSourceFile>>,
        table: &Table,
        context: &Arc<SnapshotContext>,

        limits: LocalGraphLimits,
        work: &Arc<Work>,
    ) -> Result<Self> {
        let mut chunks = Vec::new();
        let mut progress = Vec::new();
        let count = table
            .row_groups
            .len()
            .checked_mul(table.fields.len())
            .ok_or(DataReadError::LimitExceeded("decoder chunks"))?;
        chunks
            .try_reserve_exact(count)
            .map_err(|_| DataReadError::LimitExceeded("decoder chunks"))?;
        progress
            .try_reserve_exact(count)
            .map_err(|_| DataReadError::LimitExceeded("decoder progress"))?;
        for chunk in table
            .chunks
            .iter()
            .filter(|chunk| table.row_groups.binary_search(&chunk.group).is_ok())
        {
            progress.push(ChunkProgress {
                next: chunk.start,
                values: 0,
                pages: 0,
            });
            chunks.push(Chunk {
                group: chunk.group,
                start: chunk.start,
                end: chunk.end,
                values: chunk.values,
                name: crate::copy_text(&chunk.name)?,
            });
        }
        Ok(Self(Arc::new(Inner {
            file: file.into(),
            length: table.length,
            footer_start: table.footer_start,
            footer: table.footer.clone(),
            trailer: table.trailer.clone(),
            chunks,
            context: Arc::clone(context),

            limits,
            work: Arc::clone(work),

            cache: Mutex::new(Cache {
                frames: Vec::new(),
                progress,
                error: None,
            }),
        })))
    }
    fn lock(&self) -> Result<MutexGuard<'_, Cache>> {
        self.0.context.check()?;
        self.0.cache.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => DataReadError::Busy,
            TryLockError::Poisoned(_) => invalid("cache poisoned"),
        })
    }
    fn failure(&self, message: &'static str) -> DataReadError {
        match self.0.cache.try_lock() {
            Ok(mut cache) => cache.error.take().unwrap_or_else(|| invalid(message)),
            Err(_) => DataReadError::Busy,
        }
    }
    fn sdk<T>(&self, result: Result<T>) -> parquet::errors::Result<T> {
        result.map_err(|error| {
            if let Ok(mut cache) = self.0.cache.try_lock()
                && cache.error.is_none()
            {
                cache.error = Some(error);
            }
            ParquetError::General("captured graph admission failed".into())
        })
    }
    fn read(&self, start: u64) -> Result<Bytes> {
        let mut cache = self.lock()?;
        self.0.work.page()?;
        if start == self.0.length - 8 {
            return Ok(self.0.trailer.clone());
        }
        if let Some(frame) = cache
            .frames
            .iter()
            .find(|frame| frame.start == start || frame.body == start)
        {
            return Ok(frame.bytes.slice((start - frame.start) as usize..));
        }
        let chunk_index = self
            .0
            .chunks
            .iter()
            .enumerate()
            .find(|(index, chunk)| cache.progress[*index].next == start && start < chunk.end)
            .map(|(index, _)| index)
            .ok_or_else(|| invalid("unadmitted SDK offset"))?;
        let chunk = &self.0.chunks[chunk_index];
        self.0.work.page()?;
        if cache.progress[chunk_index].pages >= self.0.limits.max_pages {
            return Err(DataReadError::LimitExceeded("pages"));
        }

        let probe_len = (chunk.end - start).min(self.0.limits.max_header_bytes as u64) as usize;

        let probe = self.0.file.read_range(start, probe_len)?;
        let (header, header_len) = protocol::header(probe.as_ref(), self.0.limits)?;
        let (compressed, _, values) = validate_page(&header, self.0.limits)?;
        let frame_len = add(header_len, compressed, "page frame")?;
        let end = start
            .checked_add(frame_len as u64)
            .filter(|end| *end <= chunk.end)
            .ok_or_else(|| invalid("page span"))?;
        let bytes = self.0.file.read_range(start, frame_len)?;
        self.0.work.page()?;
        let (final_header, final_len) = protocol::header(bytes.as_ref(), self.0.limits)?;
        if final_header != header || final_len != header_len {
            return Err(invalid("page changed after probe"));
        }
        let total_values = cache.progress[chunk_index]
            .values
            .checked_add(values)
            .filter(|n| *n <= chunk.values)
            .ok_or_else(|| invalid("page value count"))?;
        let bytes = Bytes::from_owner(bytes);
        cache
            .frames
            .try_reserve(1)
            .map_err(|_| DataReadError::LimitExceeded("cache metadata"))?;
        cache.frames.push(Frame {
            start,
            body: start + header_len as u64,
            end,
            bytes: bytes.clone(),
            chunk: chunk_index,
        });
        cache.progress[chunk_index].values = total_values;
        cache.progress[chunk_index].pages += 1;
        Ok(bytes)
    }
    fn bytes(&self, start: u64, length: usize) -> Result<Bytes> {
        let mut cache = self.lock()?;
        if start == self.0.footer_start && length == self.0.footer.len() {
            return Ok(self.0.footer.clone());
        }
        if let Some((index, chunk)) = self.0.chunks.iter().enumerate().find(|(_, chunk)| {
            chunk.start == start && chunk.end.checked_sub(start) == Some(length as u64)
        }) {
            let bytes = self.0.file.read_range(start, length)?;
            let mut offset = 0usize;
            let mut values = 0u64;
            let mut pages = 0usize;
            while offset < bytes.as_ref().len() {
                self.0.context.check()?;
                pages = add(pages, 1, "chunk pages")?;
                bounded(pages, self.0.limits.max_pages, "chunk pages")?;
                self.0.work.page()?;
                let end = offset
                    .saturating_add(self.0.limits.max_header_bytes)
                    .min(bytes.as_ref().len());
                let (header, header_bytes) =
                    protocol::header(&bytes.as_ref()[offset..end], self.0.limits)?;
                let (compressed, _, count) = validate_page(&header, self.0.limits)?;
                offset = offset
                    .checked_add(header_bytes)
                    .and_then(|offset| offset.checked_add(compressed))
                    .filter(|offset| *offset <= bytes.as_ref().len())
                    .ok_or_else(|| invalid("page span"))?;
                values = values
                    .checked_add(count)
                    .filter(|values| *values <= chunk.values)
                    .ok_or_else(|| invalid("page value count"))?;
            }
            if values != chunk.values {
                return Err(invalid("page value total"));
            }
            cache.progress[index].next = chunk.end;
            cache.progress[index].values = values;
            cache.progress[index].pages = pages;
            return Ok(Bytes::from_owner(bytes));
        }
        let index = cache
            .frames
            .iter()
            .position(|frame| frame.body == start && frame.end - start == length as u64)
            .ok_or_else(|| invalid("unadmitted SDK range"))?;
        let frame = cache.frames.remove(index);
        cache.progress[frame.chunk].next = frame.end;
        Ok(frame.bytes.slice((start - frame.start) as usize..))
    }
    fn complete_group(&self, group: usize, names: &[&str]) -> Result<()> {
        let cache = self.lock()?;
        for (chunk, progress) in self.0.chunks.iter().zip(&cache.progress) {
            if chunk.group == group
                && names.contains(&chunk.name.as_str())
                && (progress.next != chunk.end || progress.values != chunk.values)
            {
                return Err(invalid("page span/value total"));
            }
        }
        Ok(())
    }
}
impl Length for Adapter {
    fn len(&self) -> u64 {
        self.0.length
    }
}
impl ChunkReader for Adapter {
    type T = Cursor<Bytes>;
    fn get_read(&self, start: u64) -> parquet::errors::Result<Self::T> {
        self.sdk(self.read(start).map(Cursor::new))
    }
    fn get_bytes(&self, start: u64, length: usize) -> parquet::errors::Result<Bytes> {
        self.sdk(self.bytes(start, length))
    }
}
pub(crate) fn validate_page(
    header: &format::PageHeader,
    limits: LocalGraphLimits,
) -> Result<(usize, usize, u64)> {
    if header.type_ != PageType::DATA_PAGE
        || header.dictionary_page_header.is_some()
        || header.data_page_header_v2.is_some()
        || header.index_page_header.is_some()
    {
        return Err(DataReadError::UnsupportedFormat("DataPageV1 only"));
    }
    let compressed = usize::try_from(header.compressed_page_size)
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("compressed page"))?;
    let uncompressed = usize::try_from(header.uncompressed_page_size)
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("uncompressed page"))?;
    bounded(
        compressed,
        limits.max_page_compressed_bytes,
        "compressed page",
    )?;
    bounded(
        uncompressed,
        limits.max_page_uncompressed_bytes,
        "uncompressed page",
    )?;
    let page = header
        .data_page_header
        .as_ref()
        .ok_or_else(|| invalid("data page header"))?;
    if page.encoding != Encoding::PLAIN
        || !matches!(
            page.definition_level_encoding,
            Encoding::RLE | Encoding::BIT_PACKED
        )
        || !matches!(
            page.repetition_level_encoding,
            Encoding::RLE | Encoding::BIT_PACKED
        )
    {
        return Err(DataReadError::UnsupportedFormat("page encoding"));
    }
    let values = u64::try_from(page.num_values)
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("page values"))?;
    Ok((compressed, uncompressed, values))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_statistics_use_modern_bounds_and_preserve_unknown_ranges() {
        use parquet::{
            data_type::ByteArray,
            file::statistics::{Statistics, ValueStatistics},
        };
        let cases = [
            (
                LookupKey::String("f0"),
                Statistics::byte_array(
                    Some(ByteArray::from("z")),
                    Some(ByteArray::from("a")),
                    None,
                    Some(0),
                    false,
                ),
            ),
            (
                LookupKey::Int64(0),
                Statistics::int64(Some(1), Some(-1), None, Some(0), false),
            ),
            (
                LookupKey::UInt64(0),
                Statistics::int64(Some(-1), Some(0), None, Some(0), false),
            ),
        ];
        for (key, statistics) in cases {
            assert!(matches!(
                key.may_match(Some(&statistics)),
                Err(DataReadError::InvalidParquet("statistics bounds"))
            ));
            assert!(key.may_match(None).unwrap());
        }
        let legacy = Statistics::int64(Some(1), Some(-1), None, Some(0), true);
        assert!(LookupKey::Int64(0).may_match(Some(&legacy)).unwrap());
        let inexact = Statistics::from(
            ValueStatistics::new(Some(1i64), Some(-1), None, Some(0), false)
                .with_min_is_exact(false),
        );
        assert!(matches!(
            LookupKey::Int64(0).may_match(Some(&inexact)),
            Err(DataReadError::InvalidParquet("statistics bounds"))
        ));
        let truncated = Statistics::from(
            ValueStatistics::new(
                Some(ByteArray::from("aaaaaaaa")),
                Some(ByteArray::from("aaaaaaab")),
                None,
                Some(0),
                false,
            )
            .with_min_is_exact(false)
            .with_max_is_exact(false),
        );
        assert!(
            LookupKey::String("aaaaaaaa-long-key")
                .may_match(Some(&truncated))
                .unwrap()
        );
        assert!(
            !LookupKey::String("zzzzzzzz-long-key")
                .may_match(Some(&truncated))
                .unwrap()
        );
        let exact = Statistics::byte_array(
            Some(ByteArray::from("a")),
            Some(ByteArray::from("z")),
            None,
            Some(0),
            false,
        );
        assert!(LookupKey::String("f0").may_match(Some(&exact)).unwrap());
        assert!(!LookupKey::String("0").may_match(Some(&exact)).unwrap());
    }
    use crate::tests::{Values, capture, context, fixture, policy, sample, writer};
    use parquet::file::serialized_reader::SerializedFileReader;
    use polars_parquet_format::thrift::protocol::TCompactOutputProtocol;
    use std::{fs, io::Read};

    #[test]
    fn selected_rows_keep_sparse_ids_nulls_and_whole_group_work() {
        let dir = crate::storage_tests::physical_slice_fixture();
        let ctx = context();
        let graph = crate::CapturedLocalGraph::capture_generated_metadata(
            dir.path(),
            Arc::clone(&ctx),
            crate::LocalGraphPolicy::new(policy()).unwrap(),
        )
        .unwrap();
        let decode = sample().into();
        let work = Work::new(decode);
        let request = DecodeRequest {
            source: &graph.reader,
            context: &ctx,
            limits: policy(),
            decode,
            work: &work,
        };
        let index = graph
            .manifest
            .node_tables
            .iter()
            .position(|node| node.label == "Function")
            .unwrap();
        let table = &graph.nodes[index];
        let mut observed = Vec::new();
        table
            .scan_selected(&request, &["added", "uid"], &[101, 300], |row, record| {
                observed.push((
                    row,
                    record
                        .get_column_iter()
                        .map(|(name, field)| (name.clone(), field.clone()))
                        .collect::<Vec<_>>(),
                ));
                Ok(())
            })
            .unwrap();
        assert_eq!(
            observed,
            vec![
                (
                    101,
                    vec![
                        ("added".into(), Field::Null),
                        ("uid".into(), Field::Str("f1".into()))
                    ]
                ),
                (
                    300,
                    vec![
                        ("added".into(), Field::Bool(true)),
                        ("uid".into(), Field::Str("f3".into()))
                    ]
                ),
            ]
        );
        assert_eq!(work.counts().0, 4);
        let before = work.counts();
        table
            .scan_selected(&request, &["uid"], &[], |_, _| {
                panic!("empty selection delivered a row")
            })
            .unwrap();
        assert_eq!(work.counts(), before);
        for rows in [&[300, 101][..], &[101, 101][..], &[400][..]] {
            assert!(
                table
                    .scan_selected(&request, &["uid"], rows, |_, _| Ok(()))
                    .is_err()
            );
        }
        assert_eq!(work.counts(), before);
        let cancellation = graphit_core::file_lock::Cancellation::default();
        cancellation.cancel();
        let cancelled = Arc::new(crate::SnapshotContext::new(
            std::time::Instant::now() + std::time::Duration::from_secs(60),
            cancellation,
        ));
        let request = DecodeRequest {
            context: &cancelled,
            ..request
        };
        assert!(
            table
                .scan_selected(&request, &["uid"], &[], |_, _| Ok(()))
                .is_err()
        );
    }

    #[test]
    fn selected_rows_skip_unselected_shared_body_but_validate_the_read_group() {
        for bad_group in [0, 1] {
            let dir = crate::storage_tests::physical_slice_fixture();
            crate::tests::corrupt_group_body(
                &dir.path().join("shards/ab/slices/Function.parquet"),
                bad_group,
                0,
            );
            let ctx = context();
            let graph = crate::CapturedLocalGraph::capture_generated_metadata(
                dir.path(),
                Arc::clone(&ctx),
                crate::LocalGraphPolicy::new(policy()).unwrap(),
            )
            .unwrap();
            let decode = sample().into();
            let work = Work::new(decode);
            let request = DecodeRequest {
                source: &graph.reader,
                context: &ctx,
                limits: policy(),
                decode,
                work: &work,
            };
            let index = graph
                .manifest
                .node_tables
                .iter()
                .position(|node| node.label == "Function")
                .unwrap();
            let table = &graph.nodes[index];
            let mut rows = Vec::new();
            let result = table.scan_selected(&request, &["uid"], &[101], |row, _| {
                rows.push(row);
                Ok(())
            });
            if bad_group == 0 {
                assert!(result.is_err());
            } else {
                result.unwrap();
                assert_eq!(rows, [101]);
                assert_eq!(work.counts().0, 3);
                assert!(
                    table
                        .scan_selected(&request, &["uid"], &[300], |_, _| Ok(()))
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn selected_rows_reject_corrupt_later_page_after_selected_row() {
        use parquet::{
            basic::Compression,
            data_type::{ByteArray, ByteArrayType},
            file::{
                metadata::KeyValue,
                properties::{WriterProperties, WriterVersion},
                writer::SerializedFileWriter,
            },
            schema::parser::parse_message_type,
        };
        let dir = fixture();
        let path = dir.path().join("Function.parquet");
        let props = WriterProperties::builder()
            .set_writer_version(WriterVersion::PARQUET_1_0)
            .set_dictionary_enabled(false)
            .set_compression(Compression::ZSTD(Default::default()))
            .set_data_page_row_count_limit(1)
            .set_write_batch_size(1)
            .set_key_value_metadata(Some(vec![KeyValue::new(
                "icebug_disk_version".into(),
                Some("v1".into()),
            )]))
            .build();
        let schema = parse_message_type(
            "message graph { required binary uid (UTF8); required binary name (UTF8); }",
        )
        .unwrap();
        let mut file = SerializedFileWriter::new(
            fs::File::create(&path).unwrap(),
            Arc::new(schema),
            Arc::new(props),
        )
        .unwrap();
        let mut group = file.next_row_group().unwrap();
        for values in [["f0", "f1", "f2"], ["alpha", "beta", "gamma"]] {
            let mut column = group.next_column().unwrap().unwrap();
            let values = values.into_iter().map(ByteArray::from).collect::<Vec<_>>();
            column
                .typed::<ByteArrayType>()
                .write_batch(&values, None, None)
                .unwrap();
            column.close().unwrap();
        }
        group.close().unwrap();
        file.close().unwrap();
        let (mut bytes, _, metadata) = raw_footer(&path);
        let chunk = metadata.row_groups[0].columns[0]
            .meta_data
            .as_ref()
            .unwrap();
        let mut offset = usize::try_from(chunk.data_page_offset).unwrap();
        for page in 0..3 {
            let (header, len) = protocol::header(&bytes[offset..], policy()).unwrap();
            assert_eq!(header.data_page_header.as_ref().unwrap().num_values, 1);
            let body = offset + len;
            if page == 2 {
                bytes[body] ^= 0xff;
            }
            offset = body + usize::try_from(header.compressed_page_size).unwrap();
        }
        fs::write(path, bytes).unwrap();
        let ctx = context();
        let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
        let decode = sample().into();
        let work = Work::new(decode);
        let request = DecodeRequest {
            source: &graph.reader,
            context: &ctx,
            limits: policy(),
            decode,
            work: &work,
        };
        let index = graph
            .manifest
            .node_tables
            .iter()
            .position(|node| node.label == "Function")
            .unwrap();
        assert!(
            graph.nodes[index]
                .scan_selected(&request, &["uid"], &[0], |_, _| Ok(()))
                .is_err()
        );
    }

    #[test]
    fn shard_projection_keeps_order_sparse_rows_nulls_and_eof_for_shared_slices() {
        for (dir, generated) in [
            (crate::tests::shard_fixture(), false),
            (crate::storage_tests::physical_slice_fixture(), true),
        ] {
            let ctx = context();
            let graph = if generated {
                crate::CapturedLocalGraph::capture_generated_metadata(
                    dir.path(),
                    Arc::clone(&ctx),
                    crate::LocalGraphPolicy::new(policy()).unwrap(),
                )
                .unwrap()
            } else {
                capture(dir.path(), Arc::clone(&ctx), policy()).unwrap()
            };
            let decode: DecodeLimits = sample().into();
            let work = Work::new(decode);
            let request = DecodeRequest {
                source: &graph.reader,
                context: &ctx,
                limits: policy(),
                decode,
                work: &work,
            };
            let table = graph
                .manifest
                .node_tables
                .iter()
                .position(|node| node.label == "Function")
                .unwrap();
            let mut observed = Vec::new();
            graph.nodes[table]
                .scan(&request, &["added", "uid", "name"], |row, record| {
                    assert_eq!(
                        record
                            .get_column_iter()
                            .map(|(name, _)| name.as_str())
                            .collect::<Vec<_>>(),
                        ["added", "uid", "name"]
                    );
                    observed.push((
                        row,
                        record
                            .get_column_iter()
                            .map(|(_, value)| value.clone())
                            .collect::<Vec<_>>(),
                    ));
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                observed.iter().map(|(row, _)| *row).collect::<Vec<_>>(),
                [100, 101, 102, 300]
            );
            for (index, (_, values)) in observed.iter().enumerate() {
                assert_eq!(
                    values[0],
                    if index < 3 {
                        Field::Null
                    } else {
                        Field::Bool(true)
                    }
                );
                assert_eq!(values[1], Field::Str(format!("f{index}")));
                assert_eq!(
                    values[2],
                    Field::Str(["alpha", "beta", "gamma", "delta"][index].into())
                );
            }
        }
    }

    fn raw_footer(path: &std::path::Path) -> (Vec<u8>, usize, format::FileMetaData) {
        let bytes = fs::read(path).unwrap();
        let len = u32::from_le_bytes(bytes[bytes.len() - 8..bytes.len() - 4].try_into().unwrap())
            as usize;
        let start = bytes.len() - 8 - len;
        let footer = protocol::footer(&bytes[start..bytes.len() - 8], policy()).unwrap();
        (bytes, start, footer)
    }
    fn replace_footer(path: &std::path::Path, change: impl FnOnce(&mut format::FileMetaData)) {
        let (mut bytes, start, mut footer) = raw_footer(path);
        change(&mut footer);
        let mut encoded = Vec::new();
        footer
            .write_to_out_protocol(&mut TCompactOutputProtocol::new(&mut encoded))
            .unwrap();
        bytes.truncate(start);
        bytes.extend_from_slice(&encoded);
        bytes.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"PAR1");
        fs::write(path, bytes).unwrap();
    }
    fn replace_page(
        path: &std::path::Path,
        change: impl FnOnce(&mut format::PageHeader, &mut Vec<u8>),
    ) {
        // A single-column genuine SDK file. Generated format writer maintains all offsets.
        let (bytes, _, mut footer) = raw_footer(path);
        assert_eq!(footer.row_groups.len(), 1);
        assert_eq!(footer.row_groups[0].columns.len(), 1);
        let column = footer.row_groups[0].columns[0].meta_data.as_mut().unwrap();
        let start = column.data_page_offset as usize;
        let (mut header, header_len) = protocol::header(&bytes[start..], policy()).unwrap();
        let mut body = bytes
            [start + header_len..start + header_len + header.compressed_page_size as usize]
            .to_vec();
        change(&mut header, &mut body);
        header.compressed_page_size = body.len() as i32;
        let mut encoded = Vec::new();
        header
            .write_to_out_protocol(&mut TCompactOutputProtocol::new(&mut encoded))
            .unwrap();
        column.total_compressed_size = (encoded.len() + body.len()) as i64;
        column.total_uncompressed_size =
            (encoded.len() + header.uncompressed_page_size as usize) as i64;
        let mut file = bytes[..start].to_vec();
        file.extend_from_slice(&encoded);
        file.extend_from_slice(&body);
        let mut tail = Vec::new();
        footer
            .write_to_out_protocol(&mut TCompactOutputProtocol::new(&mut tail))
            .unwrap();
        file.extend_from_slice(&tail);
        file.extend_from_slice(&(tail.len() as u32).to_le_bytes());
        file.extend_from_slice(b"PAR1");
        fs::write(path, file).unwrap();
    }
    fn single_node(path: &std::path::Path) {
        let mut manifest = crate::tests::manifest();
        manifest["node_tables"] = serde_json::json!([{
            "label":"File","rows":2,"shards":[{"file":"one.parquet","row_base":0,"rows":2}],"primary_key":"uid",
            "columns":[{"name":"uid","type":"STRING"}],"lang_counts":[]}]);
        crate::tests::save(path, &manifest);
        writer(
            &path.join("one.parquet"),
            "message graph { required binary uid (UTF8); }",
            &[Values::String(&["one", "two"])],
        );
    }

    fn footer_with_spans(
        template: &format::FileMetaData,
        spans: &[(u64, u64)],
    ) -> format::FileMetaData {
        let mut footer = template.clone();
        footer.row_groups = spans
            .iter()
            .map(|&(start, end)| {
                let mut group = template.row_groups[0].clone();
                group.num_rows = i64::from(start != end);
                let data = group.columns[0].meta_data.as_mut().unwrap();
                data.num_values = group.num_rows;
                data.data_page_offset = start.try_into().unwrap();
                data.total_compressed_size = (end - start).try_into().unwrap();
                group
            })
            .collect();
        footer.num_rows = footer.row_groups.iter().map(|group| group.num_rows).sum();
        footer
    }

    #[test]
    fn chunk_spans_validate_half_open_ranges_in_any_order() {
        let dir = tempfile::tempdir().unwrap();
        single_node(dir.path());
        let (_, _, template) = raw_footer(&dir.path().join("one.parquet"));
        let columns = [Column {
            name: "uid".into(),
            kind: "STRING".into(),
        }];
        for (spans, overlaps) in [
            ([(4, 6), (6, 8), (5, 5)], false), // Adjacent and an interior empty range.
            ([(4, 4), (4, 8), (8, 8)], false), // Empty ranges at either boundary.
            ([(6, 6), (6, 6), (6, 6)], false), // Repeated empty positions.
            ([(4, 5), (7, 8), (5, 7)], false), // Physical order may differ.
            ([(4, 8), (4, 8), (6, 6)], true),  // Duplicate nonempty ranges.
            ([(4, 8), (5, 7), (8, 8)], true),  // Nested nonempty ranges.
            ([(4, 6), (5, 8), (4, 4)], true),  // Partial overlap.
        ] {
            for order in [
                [0, 1, 2],
                [0, 2, 1],
                [1, 0, 2],
                [1, 2, 0],
                [2, 0, 1],
                [2, 1, 0],
            ] {
                let spans = order.map(|index| spans[index]);
                let footer = footer_with_spans(&template, &spans);
                let result = validate(
                    &footer,
                    footer.num_rows as u64,
                    Shape::Nodes(&columns),
                    8,
                    policy(),
                );
                if overlaps {
                    assert!(
                        matches!(
                            result,
                            Err(DataReadError::InvalidParquet("overlapping chunks"))
                        ),
                        "{spans:?}"
                    );
                } else {
                    let validated = result.unwrap();
                    assert_eq!(
                        validated
                            .chunks
                            .iter()
                            .map(|chunk| (chunk.start, chunk.end))
                            .collect::<Vec<_>>(),
                        spans
                    );
                    assert_eq!(
                        validated
                            .chunks
                            .iter()
                            .map(|chunk| chunk.group)
                            .collect::<Vec<_>>(),
                        [0, 1, 2]
                    );
                }
            }
        }
    }

    #[test]
    fn chunk_spans_keep_bounds_and_reject_real_overlaps() {
        let dir = tempfile::tempdir().unwrap();
        single_node(dir.path());
        let (_, _, template) = raw_footer(&dir.path().join("one.parquet"));
        let columns = [Column {
            name: "uid".into(),
            kind: "STRING".into(),
        }];
        let check = |footer: &format::FileMetaData, limits| {
            validate(
                footer,
                footer.num_rows as u64,
                Shape::Nodes(&columns),
                16,
                limits,
            )
        };
        for spans in [&[(3, 4)][..], &[(15, 17)], &[(17, 17)]] {
            assert!(matches!(
                check(&footer_with_spans(&template, spans), policy()),
                Err(DataReadError::InvalidParquet("chunk span"))
            ));
        }
        let footer = footer_with_spans(&template, &[(10, 16), (4, 10), (16, 16)]);
        let mut limits = policy();
        limits.max_chunk_bytes = 6;
        assert!(check(&footer, limits).is_ok());
        limits.max_chunk_bytes = 5;
        assert!(matches!(
            check(&footer, limits),
            Err(DataReadError::LimitExceeded("chunk bytes"))
        ));

        let mut footer = footer_with_spans(&template, &[(4, 10), (9, 12), (12, 14)]);
        assert!(matches!(
            check(&footer, policy()),
            Err(DataReadError::InvalidParquet("overlapping chunks"))
        ));
        footer.row_groups[1].columns[0]
            .meta_data
            .as_mut()
            .unwrap()
            .total_compressed_size = 0;
        assert!(matches!(
            check(&footer, policy()),
            Err(DataReadError::InvalidParquet("empty chunk"))
        ));
        footer.row_groups[1].columns[0]
            .meta_data
            .as_mut()
            .unwrap()
            .total_compressed_size = 8;
        assert!(matches!(
            check(&footer, policy()),
            Err(DataReadError::InvalidParquet("chunk span"))
        ));
    }

    #[test]
    fn sdk_footer_header_body_peek_share_exact_admitted_cache() {
        let dir = fixture();
        let ctx = context();
        let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
        let table = &graph.members[0].indices;
        let adapter = Adapter::new(
            graph
                .reader
                .open_file(relative(&table.path).unwrap())
                .unwrap(),
            table,
            &ctx,
            policy(),
            &Work::new(sample().into()),
        )
        .unwrap();

        let mut read = adapter.get_read(table.length - 8).unwrap();
        let mut trailer = [0; 8];
        read.read_exact(&mut trailer).unwrap();
        assert_eq!(trailer.as_slice(), table.trailer.as_ref());
        assert_eq!(
            adapter
                .get_bytes(table.footer_start, table.footer.len())
                .unwrap(),
            table.footer
        );

        let start = table.chunks[0].start;
        let cached = adapter.read(start).unwrap();
        let (_, header_len) = protocol::header(&cached, policy()).unwrap();
        let body_start = start + header_len as u64;

        let mut again = adapter.get_read(start).unwrap();
        let mut bytes = Vec::new();
        again.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, cached);
        let mut suffix = adapter.get_read(body_start).unwrap();
        let mut body = Vec::new();
        suffix.read_to_end(&mut body).unwrap();
        assert_eq!(body, &cached[header_len..]);
        let admitted = adapter.get_bytes(body_start, body.len()).unwrap();
        assert_eq!(admitted, body);

        assert!(adapter.get_bytes(body_start, body.len()).is_err());
        assert!(adapter.get_read(1).is_err());
    }
    #[test]
    fn mutation_after_frame_admission_cannot_mix_new_bytes() {
        let dir = fixture();
        let ctx = context();
        let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
        let table = &graph.members[0].indices;
        let adapter = Adapter::new(
            graph
                .reader
                .open_file(relative(&table.path).unwrap())
                .unwrap(),
            table,
            &ctx,
            policy(),
            &Work::new(sample().into()),
        )
        .unwrap();
        let start = table.chunks[0].start;
        let cached = adapter.read(start).unwrap();
        let (_, header_len) = protocol::header(&cached, policy()).unwrap();
        let path = dir.path().join(table.relative_path());
        let mut changed = fs::read(&path).unwrap();
        changed[start as usize + header_len] ^= 0xff;
        fs::write(path, changed).unwrap();
        let admitted = adapter
            .get_bytes(start + header_len as u64, cached.len() - header_len)
            .unwrap();
        assert_eq!(admitted, &cached[header_len..]);
        assert!(graph.reader.revalidate_content().is_err());
        assert!(graph.sample_graph(sample()).is_err());
    }
    #[test]
    fn malformed_schema_annotations_offsets_and_external_columns_fail_before_sdk() {
        for variant in 0..9 {
            let dir = fixture();
            let path = dir.path().join("Function.parquet");
            replace_footer(&path, |footer| match variant {
                0 => footer.schema[0].num_children = Some(i32::MAX),
                1 => footer.schema[1].repetition_type = Some(FieldRepetitionType::REPEATED),
                2 => {
                    footer.schema[1].logical_type = Some(LogicalType::JSON(format::JsonType::new()))
                }
                3 => footer.row_groups[0].columns[0].file_path = Some("external.parquet".into()),
                4 => {
                    footer.row_groups[0].columns[0]
                        .meta_data
                        .as_mut()
                        .unwrap()
                        .data_page_offset = -1
                }
                5 => {
                    footer.row_groups[0].columns[0]
                        .meta_data
                        .as_mut()
                        .unwrap()
                        .total_compressed_size = i64::MAX
                }
                6 => {
                    footer.row_groups[0].columns[0]
                        .meta_data
                        .as_mut()
                        .unwrap()
                        .codec = CompressionCodec::UNCOMPRESSED
                }
                7 => {
                    footer.row_groups[0].columns[0]
                        .meta_data
                        .as_mut()
                        .unwrap()
                        .dictionary_page_offset = Some(4)
                }
                _ => {
                    footer.row_groups[0].columns[0]
                        .meta_data
                        .as_mut()
                        .unwrap()
                        .num_values = -1
                }
            });
            assert!(
                capture(dir.path(), context(), policy()).is_err(),
                "variant {variant}"
            );
        }
    }
    #[test]
    fn bounded_page_c_u_and_wrong_v2_dictionary_encoding_fail_closed() {
        for variant in 0..6 {
            let dir = tempfile::tempdir().unwrap();
            single_node(dir.path());
            let path = dir.path().join("one.parquet");
            replace_page(&path, |header, _| match variant {
                0 => header.type_ = PageType::DICTIONARY_PAGE,
                1 => header.type_ = PageType::DATA_PAGE_V2,
                2 => header.uncompressed_page_size = i32::MAX,
                3 => header.data_page_header.as_mut().unwrap().encoding = Encoding::RLE_DICTIONARY,
                4 => header.data_page_header.as_mut().unwrap().num_values = -1,
                _ => header.data_page_header.as_mut().unwrap().num_values = 3,
            });
            let graph = capture(dir.path(), context(), policy()).unwrap();
            assert!(graph.sample_graph(sample()).is_err(), "variant {variant}");
        }
    }
    #[test]
    fn genuine_zstd_exact_inflation_undersize_corrupt_and_truncated_bodies() {
        for variant in 0..5 {
            let dir = tempfile::tempdir().unwrap();
            single_node(dir.path());
            let path = dir.path().join("one.parquet");
            if variant != 0 {
                replace_page(&path, |header, body| match variant {
                    1 => header.uncompressed_page_size = 1,
                    2 => header.uncompressed_page_size += 1,
                    3 => body[0] ^= 0xff,
                    _ => {
                        body.pop();
                    }
                });
            }
            let graph = capture(dir.path(), context(), policy()).unwrap();
            let result = graph.sample_graph(sample());
            assert_eq!(result.is_ok(), variant == 0, "variant {variant}");
            if let Err(error) = result {
                assert!(matches!(error, DataReadError::InvalidParquet(_)));
            }
        }
    }
    #[test]
    fn malformed_plain_string_lengths_are_typed_errors_without_panic_mask() {
        let dir = tempfile::tempdir().unwrap();
        single_node(dir.path());
        let path = dir.path().join("one.parquet");
        replace_page(&path, |header, body| {
            let mut plain =
                zstd::bulk::decompress(body, header.uncompressed_page_size as usize).unwrap();
            plain[..4].copy_from_slice(&u32::MAX.to_le_bytes());
            *body = zstd::bulk::compress(&plain, 0).unwrap();
        });
        let graph = capture(dir.path(), context(), policy()).unwrap();
        assert!(matches!(
            graph.sample_graph(sample()),
            Err(DataReadError::InvalidParquet(_))
        ));
    }
    #[test]
    fn malformed_nullable_levels_are_typed_errors_without_panic_mask() {
        let dir = tempfile::tempdir().unwrap();
        single_node(dir.path());
        let path = dir.path().join("one.parquet");
        writer(
            &path,
            "message graph { optional binary uid (UTF8); }",
            &[Values::OptionalString(&[Some("one"), Some("two")])],
        );
        replace_page(&path, |header, body| {
            let mut plain =
                zstd::bulk::decompress(body, header.uncompressed_page_size as usize).unwrap();
            plain[..4].copy_from_slice(&u32::MAX.to_le_bytes());
            *body = zstd::bulk::compress(&plain, 0).unwrap();
        });
        let graph = capture(dir.path(), context(), policy()).unwrap();
        assert!(matches!(
            graph.sample_graph(sample()),
            Err(DataReadError::InvalidParquet(_))
        ));
    }
    #[test]
    fn strict_utf8_decoder_error_does_not_retain_raw_payload() {
        let dir = tempfile::tempdir().unwrap();
        single_node(dir.path());
        let path = dir.path().join("one.parquet");
        replace_page(&path, |header, body| {
            let mut plain =
                zstd::bulk::decompress(body, header.uncompressed_page_size as usize).unwrap();
            plain[4] = 0xff;
            *body = zstd::bulk::compress(&plain, 0).unwrap();
        });
        let graph = capture(dir.path(), context(), policy()).unwrap();
        let failure = graph.sample_graph(sample()).err().unwrap();
        assert_eq!(failure.to_string(), "invalid parquet: row decoder");
    }
    #[test]
    fn genuine_footer_duplicate_and_type_mismatch_differential() {
        use polars_parquet_format::thrift::protocol::{
            TFieldIdentifier, TListIdentifier, TOutputProtocol, TStructIdentifier, TType,
        };
        for wrong_type in [false, true] {
            let dir = fixture();
            let path = dir.path().join("Function.parquet");
            let (mut bytes, start, footer) = raw_footer(&path);
            let mut encoded = Vec::new();
            let mut writer = TCompactOutputProtocol::new(&mut encoded);
            writer
                .write_struct_begin(&TStructIdentifier::new("FileMetaData"))
                .unwrap();
            for _ in 0..if wrong_type { 1 } else { 2 } {
                writer
                    .write_field_begin(&TFieldIdentifier::new(
                        "version",
                        if wrong_type { TType::I64 } else { TType::I32 },
                        1,
                    ))
                    .unwrap();
                if wrong_type {
                    writer.write_i64(footer.version as i64).unwrap();
                } else {
                    writer.write_i32(footer.version).unwrap();
                }
                writer.write_field_end().unwrap();
            }
            writer
                .write_field_begin(&TFieldIdentifier::new("schema", TType::List, 2))
                .unwrap();
            writer
                .write_list_begin(&TListIdentifier::new(
                    TType::Struct,
                    footer.schema.len() as u32,
                ))
                .unwrap();
            for element in &footer.schema {
                element.write_to_out_protocol(&mut writer).unwrap();
            }
            writer.write_list_end().unwrap();
            writer.write_field_end().unwrap();
            writer
                .write_field_begin(&TFieldIdentifier::new("num_rows", TType::I64, 3))
                .unwrap();
            writer.write_i64(footer.num_rows).unwrap();
            writer.write_field_end().unwrap();
            writer
                .write_field_begin(&TFieldIdentifier::new("row_groups", TType::List, 4))
                .unwrap();
            writer
                .write_list_begin(&TListIdentifier::new(
                    TType::Struct,
                    footer.row_groups.len() as u32,
                ))
                .unwrap();
            for element in &footer.row_groups {
                element.write_to_out_protocol(&mut writer).unwrap();
            }
            writer.write_list_end().unwrap();
            writer.write_field_end().unwrap();
            let keys = footer.key_value_metadata.as_ref().unwrap();
            writer
                .write_field_begin(&TFieldIdentifier::new("key_value_metadata", TType::List, 5))
                .unwrap();
            writer
                .write_list_begin(&TListIdentifier::new(TType::Struct, keys.len() as u32))
                .unwrap();
            for element in keys {
                element.write_to_out_protocol(&mut writer).unwrap();
            }
            writer.write_list_end().unwrap();
            writer.write_field_end().unwrap();
            writer.write_field_stop().unwrap();
            writer.write_struct_end().unwrap();
            bytes.truncate(start);
            bytes.extend_from_slice(&encoded);
            bytes.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
            bytes.extend_from_slice(b"PAR1");
            fs::write(&path, bytes).unwrap();
            let raw_sdk = SerializedFileReader::new(fs::File::open(&path).unwrap());
            eprintln!(
                "footer differential wrong_type={wrong_type} raw_sdk_accepted={}",
                raw_sdk.is_ok()
            );
            assert!(
                raw_sdk.is_ok(),
                "Parquet 60 raw SDK acceptance changed for wrong_type={wrong_type}"
            );
            assert!(matches!(
                capture(dir.path(), context(), policy()),
                Err(DataReadError::InvalidParquet("footer protocol"))
            ));
        }
    }

    #[test]
    fn busy_cache_fails_without_wait_or_io_and_recovers_after_guard_drop() {
        let dir = fixture();
        let ctx = context();
        let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
        let table = &graph.members[0].indices;
        let adapter = Adapter::new(
            graph
                .reader
                .open_file(relative(&table.path).unwrap())
                .unwrap(),
            table,
            &ctx,
            policy(),
            &Work::new(sample().into()),
        )
        .unwrap();
        let guard = adapter.0.cache.try_lock().unwrap();

        assert!(matches!(
            adapter.read(table.chunks[0].start),
            Err(DataReadError::Busy)
        ));

        drop(guard);
        assert!(adapter.read(table.chunks[0].start).is_ok());
    }
    #[test]
    fn page_header_compressed_uncompressed_exact_plus_one_boundaries() {
        let dir = fixture();
        let ctx = context();
        let graph = capture(dir.path(), Arc::clone(&ctx), policy()).unwrap();
        let table = &graph.members[0].indices;
        let raw = fs::read(dir.path().join(table.relative_path())).unwrap();
        let (header, header_len) =
            protocol::header(&raw[table.chunks[0].start as usize..], policy()).unwrap();
        for variant in 0..3 {
            let mut limits = policy();
            match variant {
                0 => limits.max_header_bytes = header_len,
                1 => limits.max_page_compressed_bytes = header.compressed_page_size as usize,
                _ => limits.max_page_uncompressed_bytes = header.uncompressed_page_size as usize,
            }
            let adapter = Adapter::new(
                graph
                    .reader
                    .open_file(relative(&table.path).unwrap())
                    .unwrap(),
                table,
                &ctx,
                limits,
                &Work::new(sample().into()),
            )
            .unwrap();
            assert!(adapter.read(table.chunks[0].start).is_ok());
            drop(adapter);
            match variant {
                0 => limits.max_header_bytes -= 1,
                1 => limits.max_page_compressed_bytes -= 1,
                _ => limits.max_page_uncompressed_bytes -= 1,
            }
            let adapter = Adapter::new(
                graph
                    .reader
                    .open_file(relative(&table.path).unwrap())
                    .unwrap(),
                table,
                &ctx,
                limits,
                &Work::new(sample().into()),
            )
            .unwrap();
            assert!(
                adapter.read(table.chunks[0].start).is_err(),
                "plus-one {variant}"
            );
        }
    }
    #[test]
    fn huge_metadata_names_footer_bytes_and_encryption_marker_fail_closed() {
        for variant in 0..3 {
            let dir = fixture();
            let path = dir.path().join("Function.parquet");
            replace_footer(&path, |footer| match variant {
                0 => footer.schema[1].name = "x".repeat(257),
                1 => footer
                    .key_value_metadata
                    .as_mut()
                    .unwrap()
                    .push(format::KeyValue::new(
                        "huge".into(),
                        Some("x".repeat(65537)),
                    )),
                _ => footer.footer_signing_key_metadata = Some(vec![1]),
            });
            assert!(capture(dir.path(), context(), policy()).is_err());
        }
    }
    #[test]
    fn root_schema_annotations_and_column_metadata_aggregate_fail_closed() {
        for variant in 0..2 {
            let dir = tempfile::tempdir().unwrap();
            single_node(dir.path());
            replace_footer(&dir.path().join("one.parquet"), |footer| {
                if variant == 0 {
                    footer.schema[0].name = "x".repeat(257);
                } else {
                    footer.schema[0].converted_type = Some(ConvertedType::UTF8);
                }
            });
            assert!(capture(dir.path(), context(), policy()).is_err());
        }
        let dir = tempfile::tempdir().unwrap();
        single_node(dir.path());
        replace_footer(&dir.path().join("one.parquet"), |footer| {
            footer.row_groups[0].columns[0]
                .meta_data
                .as_mut()
                .unwrap()
                .key_value_metadata = Some(vec![format::KeyValue::new(
                "column-key".into(),
                Some("column-value".into()),
            )]);
        });
        let graph = capture(dir.path(), context(), policy()).unwrap();
        let footer = &graph.nodes[0].metadata;
        let file_bytes = footer.created_by.as_ref().map_or(0, String::len)
            + footer
                .key_value_metadata
                .iter()
                .flatten()
                .map(|item| item.key.len() + item.value.as_ref().map_or(0, String::len))
                .sum::<usize>();
        let mut limits = policy();
        limits.max_metadata_entries = 2;
        limits.max_metadata_bytes = file_bytes + "column-key".len() + "column-value".len();
        assert!(capture(dir.path(), context(), limits).is_ok());
        limits.max_metadata_entries = 1;
        assert!(matches!(
            capture(dir.path(), context(), limits),
            Err(DataReadError::LimitExceeded("metadata entries"))
        ));
        limits.max_metadata_entries = 2;
        limits.max_metadata_bytes -= 1;
        assert!(matches!(
            capture(dir.path(), context(), limits),
            Err(DataReadError::LimitExceeded("metadata bytes"))
        ));
    }
}
