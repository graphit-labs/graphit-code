//! Official SCIP Index fields, visited or written one bounded message at a time.
//!
//! Callbacks may run before metadata or a later malformed record is discovered.
//! They must stage effects; only a successful return qualifies the complete input.
//! Duplicate document paths belong to the caller's staging UNIQUE constraint.
use crate::scip::AstScipError;
use ::scip::types::{Document, Metadata, SymbolInformation, occurrence};
use graphit_core::file_io::SnapshotContext;
use protobuf::{CodedInputStream, CodedOutputStream, Message, rt::WireType};
use std::{
    io::{self, Read, Write},
    num::NonZeroU32,
};

/// Ownership of one official record; no corpus is retained by this module.
pub enum IndexRecord {
    Document(Document),
    ExternalSymbol(SymbolInformation),
}

struct CheckedRead<'a> {
    reader: &'a mut dyn Read,
    original: &'a SnapshotContext,
}
fn stream_live(original: &SnapshotContext) -> io::Result<()> {
    // Protobuf uses std read/write_all loops, which retry Interrupted. A terminal
    // request failure must stop that loop; the public function's final Original
    // check returns the actual cancellation/deadline error without reclassifying it.
    original.check().map_err(io::Error::other)
}
impl Read for CheckedRead<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        stream_live(self.original)?;
        let result = self.reader.read(buffer);
        stream_live(self.original)?;
        result
    }
}
struct CheckedWrite<'a> {
    writer: &'a mut dyn Write,
    original: &'a SnapshotContext,
}
impl Write for CheckedWrite<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        stream_live(self.original)?;
        let result = self.writer.write(bytes);
        stream_live(self.original)?;
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        stream_live(self.original)?;
        let result = self.writer.flush();
        stream_live(self.original)?;
        result
    }
}
fn invalid(reason: &'static str) -> AstScipError {
    io::Error::new(io::ErrorKind::InvalidData, reason).into()
}
fn bounded(size: u64, message_bytes: NonZeroU32) -> Result<(), AstScipError> {
    if size > u64::from(message_bytes.get()) {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            "SCIP message exceeds input limit",
        )
        .into());
    }
    Ok(())
}
fn merge_message<M: Message>(
    input: &mut CodedInputStream<'_>,
    message: &mut M,
    message_bytes: NonZeroU32,
) -> Result<(), AstScipError> {
    let size = input.read_raw_varint64().map_err(AstScipError::Protobuf)?;
    bounded(size, message_bytes)?;
    let end = input
        .pos()
        .checked_add(size)
        .ok_or_else(|| invalid("SCIP message length overflow"))?;
    let previous = input.push_limit(size).map_err(AstScipError::Protobuf)?;
    message.merge_from(input).map_err(AstScipError::Protobuf)?;
    if input.pos() != end {
        return Err(invalid("truncated SCIP message"));
    }
    message
        .check_initialized()
        .map_err(AstScipError::Protobuf)?;
    input.pop_limit(previous);
    bounded(message.compute_size(), message_bytes)
}
fn tag_parts(tag: u32) -> Result<(u32, WireType), AstScipError> {
    let field = tag >> 3;
    if field == 0 {
        return Err(invalid("zero SCIP protobuf field number"));
    }
    Ok((
        field,
        WireType::new(tag & 7).ok_or_else(|| invalid("invalid SCIP protobuf wire type"))?,
    ))
}
fn skip_scalar(
    input: &mut CodedInputStream<'_>,
    wire: WireType,
    message_bytes: NonZeroU32,
) -> Result<(), AstScipError> {
    if wire == WireType::LengthDelimited {
        let size = input.read_raw_varint64().map_err(AstScipError::Protobuf)?;
        bounded(size, message_bytes)?;
        input
            .skip_raw_bytes(size as u32)
            .map_err(AstScipError::Protobuf)?;
    } else {
        input.skip_field(wire).map_err(AstScipError::Protobuf)?;
    }
    Ok(())
}
fn skip_unknown(
    input: &mut CodedInputStream<'_>,
    field: u32,
    wire: WireType,
    original: &SnapshotContext,
    message_bytes: NonZeroU32,
) -> Result<(), AstScipError> {
    if wire == WireType::EndGroup {
        return Err(invalid("unexpected SCIP protobuf end group"));
    }
    if wire != WireType::StartGroup {
        return skip_scalar(input, wire, message_bytes);
    }
    // Iterative matching also rejects mismatched or unterminated deprecated groups.
    // The stack is bounded by the encoded size of this one unknown field.
    let start = input.pos();
    let mut groups = vec![field];
    while !groups.is_empty() {
        original.check()?;
        bounded(input.pos() - start, message_bytes)?;
        let tag = input
            .read_raw_tag_or_eof()
            .map_err(AstScipError::Protobuf)?
            .ok_or_else(|| invalid("unterminated SCIP protobuf group"))?;
        let (field, wire) = tag_parts(tag)?;
        match wire {
            WireType::StartGroup => groups.push(field),
            WireType::EndGroup => {
                if groups.pop() != Some(field) {
                    return Err(invalid("mismatched SCIP protobuf group"));
                }
            }
            _ => skip_scalar(input, wire, message_bytes)?,
        }
    }
    bounded(input.pos() - start, message_bytes)
}
fn validate_metadata(metadata: &Metadata) -> Result<(), AstScipError> {
    if metadata.version.enum_value().is_err()
        || metadata.text_document_encoding.enum_value().is_err()
    {
        return Err(invalid("unsupported SCIP metadata enum"));
    }
    let Some((scheme, rest)) = metadata.project_root.split_once(':') else {
        return Err(invalid("SCIP project root must be an absolute URI"));
    };
    if scheme.is_empty()
        || !scheme.as_bytes()[0].is_ascii_alphabetic()
        || !scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+.-".contains(&b))
        || rest.is_empty()
        || metadata
            .project_root
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return Err(invalid("SCIP project root must be an absolute URI"));
    }
    Ok(())
}
fn packed_range(range: &[i32]) -> Result<[i32; 4], AstScipError> {
    match range {
        [a, b, d] => coordinate_range(*a, *b, *a, *d),
        [a, b, c, d] => coordinate_range(*a, *b, *c, *d),
        _ => Err(invalid("SCIP range requires three or four coordinates")),
    }
}
fn coordinate_range(a: i32, b: i32, c: i32, d: i32) -> Result<[i32; 4], AstScipError> {
    if a < 0 || b < 0 || c < 0 || d < 0 || (a, b) > (c, d) {
        return Err(invalid("negative or reversed SCIP range"));
    }
    Ok([a, b, c, d])
}
fn combine_range(
    packed: &[i32],
    typed: Option<[i32; 4]>,
) -> Result<Option<[i32; 4]>, AstScipError> {
    let packed = if packed.is_empty() {
        None
    } else {
        Some(packed_range(packed)?)
    };
    if let (Some(packed), Some(typed)) = (packed, typed)
        && packed != typed
    {
        return Err(invalid("SCIP packed and typed ranges disagree"));
    }
    Ok(typed.or(packed))
}
fn validate_document(document: &Document, original: &SnapshotContext) -> Result<(), AstScipError> {
    let path = &document.relative_path;
    if path.is_empty()
        || path.starts_with('/')
        || path.contains(['\\', '\0'])
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || (path.as_bytes().get(1) == Some(&b':') && path.as_bytes()[0].is_ascii_alphabetic())
    {
        return Err(invalid("invalid SCIP relative document path"));
    }
    if document.position_encoding.enum_value().is_err() {
        return Err(invalid("unsupported SCIP position encoding"));
    }
    for occurrence in &document.occurrences {
        original.check()?;
        let typed = match &occurrence.typed_range {
            Some(occurrence::Typed_range::SingleLineRange(r)) => Some(coordinate_range(
                r.line,
                r.start_character,
                r.line,
                r.end_character,
            )?),
            Some(occurrence::Typed_range::MultiLineRange(r)) => Some(coordinate_range(
                r.start_line,
                r.start_character,
                r.end_line,
                r.end_character,
            )?),
            None => None,
            Some(_) => return Err(invalid("unsupported typed SCIP range")),
        };
        let range = combine_range(&occurrence.range, typed)?
            .ok_or_else(|| invalid("missing SCIP occurrence range"))?;
        let typed = match &occurrence.typed_enclosing_range {
            Some(occurrence::Typed_enclosing_range::SingleLineEnclosingRange(r)) => Some(
                coordinate_range(r.line, r.start_character, r.line, r.end_character)?,
            ),
            Some(occurrence::Typed_enclosing_range::MultiLineEnclosingRange(r)) => Some(
                coordinate_range(r.start_line, r.start_character, r.end_line, r.end_character)?,
            ),
            None => None,
            Some(_) => return Err(invalid("unsupported typed SCIP enclosing range")),
        };
        if let Some(enclosing) = combine_range(&occurrence.enclosing_range, typed)?
            && ((enclosing[0], enclosing[1]) > (range[0], range[1])
                || (enclosing[2], enclosing[3]) < (range[2], range[3]))
        {
            return Err(invalid("SCIP enclosing range does not contain occurrence"));
        }
    }
    original.check()?;
    Ok(())
}

/// Visit an official Index through EOF, retaining only metadata and one record.
/// `message_bytes` bounds each message/unknown length-delimited field, not the Index.
/// Known enum values, including Unspecified, are preserved without inventing encoding defaults.
pub fn visit_index(
    reader: &mut dyn Read,
    original: &SnapshotContext,
    message_bytes: NonZeroU32,
    mut visit: impl FnMut(IndexRecord) -> Result<(), AstScipError>,
) -> Result<Metadata, AstScipError> {
    original.check()?;
    let result = (|| {
        let mut checked = CheckedRead { reader, original };
        let mut input = CodedInputStream::new(&mut checked);
        let mut metadata = Metadata::new();
        let mut seen_metadata = false;
        while let Some(tag) = input
            .read_raw_tag_or_eof()
            .map_err(AstScipError::Protobuf)?
        {
            original.check()?;
            let (field, wire) = tag_parts(tag)?;
            match (field, wire) {
                (1, WireType::LengthDelimited) => {
                    // Match the pinned official decoder: a repeated singular
                    // message replaces the previous occurrence, including defaults.
                    metadata = Metadata::new();
                    merge_message(&mut input, &mut metadata, message_bytes)?;
                    seen_metadata = true;
                }
                (2, WireType::LengthDelimited) => {
                    let mut document = Document::new();
                    merge_message(&mut input, &mut document, message_bytes)?;
                    validate_document(&document, original)?;
                    visit(IndexRecord::Document(document))?;
                }
                (3, WireType::LengthDelimited) => {
                    let mut symbol = SymbolInformation::new();
                    merge_message(&mut input, &mut symbol, message_bytes)?;
                    visit(IndexRecord::ExternalSymbol(symbol))?;
                }
                _ => skip_unknown(&mut input, field, wire, original, message_bytes)?,
            }
            original.check()?;
        }
        if !seen_metadata {
            return Err(AstScipError::MissingMetadata);
        }
        validate_metadata(&metadata)?;
        Ok(metadata)
    })();
    original.check()?;
    result
}
fn write_field<M: Message>(
    writer: &mut dyn Write,
    field: u32,
    value: &M,
    original: &SnapshotContext,
    message_bytes: NonZeroU32,
) -> Result<(), AstScipError> {
    original.check()?;
    bounded(value.compute_size(), message_bytes)?;
    let result = (|| {
        let mut checked = CheckedWrite { writer, original };
        let mut output = CodedOutputStream::new(&mut checked);
        output
            .write_tag(field, WireType::LengthDelimited)
            .map_err(AstScipError::Protobuf)?;
        value
            .write_length_delimited_to(&mut output)
            .map_err(AstScipError::Protobuf)?;
        output.flush().map_err(AstScipError::Protobuf)
    })();
    original.check()?;
    result
}
/// Write field 1 of an official Index. Call once for a complete metadata value.
pub fn write_metadata(
    writer: &mut dyn Write,
    value: &Metadata,
    original: &SnapshotContext,
    message_bytes: NonZeroU32,
) -> Result<(), AstScipError> {
    original.check()?;
    validate_metadata(value)?;
    write_field(writer, 1, value, original, message_bytes)
}
/// Write field 2 without retaining any preceding document.
pub fn write_document(
    writer: &mut dyn Write,
    value: &Document,
    original: &SnapshotContext,
    message_bytes: NonZeroU32,
) -> Result<(), AstScipError> {
    original.check()?;
    validate_document(value, original)?;
    write_field(writer, 2, value, original, message_bytes)
}
/// Write field 3 of the official Index; no framing or corpus accumulator.
pub fn write_external_symbol(
    writer: &mut dyn Write,
    value: &SymbolInformation,
    original: &SnapshotContext,
    message_bytes: NonZeroU32,
) -> Result<(), AstScipError> {
    write_field(writer, 3, value, original, message_bytes)
}

#[cfg(test)]
mod tests;
