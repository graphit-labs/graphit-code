use super::*;
use ::scip::types::{Index, Occurrence, PositionEncoding, SingleLineRange, TextEncoding, ToolInfo};
use graphit_core::file_lock::Cancellation;
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn limit() -> NonZeroU32 {
    NonZeroU32::new(4096).unwrap()
}
fn metadata() -> Metadata {
    Metadata {
        project_root: "file:///workspace/%CE%BB".into(),
        text_document_encoding: TextEncoding::UTF8.into(),
        tool_info: Some(ToolInfo {
            name: "stream-test".into(),
            version: "1".into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}
fn document() -> Document {
    let mut doc = Document {
        relative_path: "src/λ😀.rs".into(),
        language: "rust".into(),
        text: "let λ = \"😀\";".into(),
        position_encoding: PositionEncoding::UTF16CodeUnitOffsetFromLineStart.into(),
        occurrences: vec![Occurrence {
            range: vec![0, 4, 5],
            symbol: "local 0".into(),
            symbol_roles: 1,
            typed_range: Some(occurrence::Typed_range::SingleLineRange(SingleLineRange {
                line: 0,
                start_character: 4,
                end_character: 5,
                ..Default::default()
            })),
            enclosing_range: vec![0, 0, 13],
            ..Default::default()
        }],
        symbols: vec![SymbolInformation {
            symbol: "local 0".into(),
            documentation: vec!["λ and 😀 documentation".into()],
            relationships: vec![::scip::types::Relationship {
                symbol: "local 1".into(),
                is_reference: true,
                is_implementation: true,
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    doc.special_fields
        .mut_unknown_fields()
        .add_length_delimited(1000, b"graphit:metrics-and-relations".to_vec());
    doc
}
fn external() -> SymbolInformation {
    SymbolInformation {
        symbol: "scip-rust cargo example 1.0 external.".into(),
        documentation: vec!["External λ".into()],
        ..Default::default()
    }
}
fn official_bytes(documents: Vec<Document>) -> Vec<u8> {
    Index {
        metadata: Some(metadata()).into(),
        documents,
        external_symbols: vec![external()],
        ..Default::default()
    }
    .write_to_bytes()
    .unwrap()
}

#[test]
fn official_to_stream_preserves_unicode_records_and_document_extensions() {
    for encoding in 0..=3 {
        let mut expected = document();
        expected.position_encoding = protobuf::EnumOrUnknown::from_i32(encoding);
        if encoding == PositionEncoding::UTF8CodeUnitOffsetFromLineStart as i32 {
            expected.occurrences[0].range = vec![0, 4, 6];
            expected.occurrences[0].typed_range =
                Some(occurrence::Typed_range::SingleLineRange(SingleLineRange {
                    line: 0,
                    start_character: 4,
                    end_character: 6,
                    ..Default::default()
                }));
        }
        let bytes = official_bytes(vec![expected.clone()]);
        let mut seen = Vec::new();
        let meta = visit_index(&mut bytes.as_slice(), &context(), limit(), |record| {
            seen.push(record);
            Ok(())
        })
        .unwrap();
        assert_eq!(meta, metadata());
        assert_eq!(seen.len(), 2);
        let IndexRecord::Document(doc) = &seen[0] else {
            panic!("document");
        };
        assert_eq!(doc, &expected);
        let IndexRecord::ExternalSymbol(symbol) = &seen[1] else {
            panic!("external");
        };
        assert_eq!(symbol, &external());
    }
}

#[test]
fn stream_to_official_has_only_index_wire_fields_and_no_private_framing() {
    let ctx = context();
    let mut output = Vec::new();
    write_metadata(&mut output, &metadata(), &ctx, limit()).unwrap();
    write_document(&mut output, &document(), &ctx, limit()).unwrap();
    write_external_symbol(&mut output, &external(), &ctx, limit()).unwrap();
    let parsed = Index::parse_from_bytes(&output).unwrap();
    assert_eq!(parsed.metadata.as_ref(), Some(&metadata()));
    assert_eq!(parsed.documents, vec![document()]);
    assert_eq!(parsed.external_symbols, vec![external()]);
    assert_eq!(output, official_bytes(vec![document()]));
}

#[test]
fn any_field_order_repeated_metadata_unknown_fields_and_groups_interoperate() {
    let ctx = context();
    let mut bytes = Vec::new();
    write_document(&mut bytes, &document(), &ctx, limit()).unwrap();
    {
        let mut out = CodedOutputStream::vec(&mut bytes);
        out.write_uint64(20, 7).unwrap();
        out.write_fixed64(21, 1).unwrap();
        out.write_fixed32(22, 2).unwrap();
        out.write_bytes(23, b"unknown").unwrap();
        out.write_tag(24, WireType::StartGroup).unwrap();
        out.write_tag(25, WireType::StartGroup).unwrap();
        out.write_string(26, "λ").unwrap();
        out.write_tag(25, WireType::EndGroup).unwrap();
        out.write_tag(24, WireType::EndGroup).unwrap();
        // The pinned official SDK replaces repeated singular metadata, including
        // defaults and unknown fields; it does not merge the two occurrences.
        out.write_message(
            1,
            &Metadata {
                version: protobuf::EnumOrUnknown::from_i32(99),
                project_root: "file:///discarded".into(),
                ..Default::default()
            },
        )
        .unwrap();
        out.write_message(1, &metadata()).unwrap();
        out.flush().unwrap();
    }
    write_external_symbol(&mut bytes, &external(), &ctx, limit()).unwrap();
    let official = Index::parse_from_bytes(&bytes).unwrap();
    let mut documents = Vec::new();
    let mut externals = Vec::new();
    let meta = visit_index(&mut bytes.as_slice(), &ctx, limit(), |record| {
        match record {
            IndexRecord::Document(doc) => documents.push(doc),
            IndexRecord::ExternalSymbol(s) => externals.push(s),
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(meta, *official.metadata.0.unwrap());
    assert_eq!(documents, official.documents);
    assert_eq!(externals, official.external_symbols);
}

#[test]
fn missing_or_invalid_metadata_fails_even_after_staged_callbacks() {
    let ctx = context();
    let mut bytes = Vec::new();
    write_document(&mut bytes, &document(), &ctx, limit()).unwrap();
    let mut count = 0;
    assert!(matches!(
        visit_index(&mut bytes.as_slice(), &ctx, limit(), |_| {
            count += 1;
            Ok(())
        }),
        Err(AstScipError::MissingMetadata)
    ));
    assert_eq!(count, 1);
    for root in [
        "",
        "relative/path",
        ":/root",
        "1bad:/root",
        "file:///a b",
        "file:///a\0b",
    ] {
        let mut meta = metadata();
        meta.project_root = root.into();
        let bytes = Index {
            metadata: Some(meta.clone()).into(),
            ..Default::default()
        }
        .write_to_bytes()
        .unwrap();
        assert!(visit_index(&mut bytes.as_slice(), &ctx, limit(), |_| Ok(())).is_err());
        let mut output = Vec::new();
        assert!(write_metadata(&mut output, &meta, &ctx, limit()).is_err());
        assert!(output.is_empty());
    }
    let mut meta = metadata();
    meta.version = protobuf::EnumOrUnknown::from_i32(99);
    assert!(write_metadata(&mut Vec::new(), &meta, &ctx, limit()).is_err());
    let mut meta = metadata();
    meta.text_document_encoding = protobuf::EnumOrUnknown::from_i32(99);
    assert!(write_metadata(&mut Vec::new(), &meta, &ctx, limit()).is_err());
}

#[test]
fn malformed_wire_truncated_messages_utf8_and_groups_fail() {
    for bytes in [
        vec![0],
        vec![14],
        vec![18, 5, 18, 0],
        vec![18, 3, 18, 1, 255],
        vec![163, 1],
        vec![163, 1, 172, 1],
        vec![164, 1],
    ] {
        assert!(
            visit_index(&mut bytes.as_slice(), &context(), limit(), |_| Ok(())).is_err(),
            "{bytes:?}"
        );
    }
    let mut bytes = official_bytes(vec![document()]);
    bytes.pop();
    assert!(visit_index(&mut bytes.as_slice(), &context(), limit(), |_| Ok(())).is_err());
}

#[test]
fn relative_paths_and_packed_typed_enclosing_ranges_are_checked() {
    let ctx = context();
    for path in [
        "",
        "/file.rs",
        "../file.rs",
        "a/./file.rs",
        "a//b",
        "a\\b",
        "C:/a",
        "a\0b",
    ] {
        let mut doc = document();
        doc.relative_path = path.into();
        let bytes = official_bytes(vec![doc.clone()]);
        assert!(visit_index(&mut bytes.as_slice(), &ctx, limit(), |_| Ok(())).is_err());
        let mut output = Vec::new();
        assert!(write_document(&mut output, &doc, &ctx, limit()).is_err());
        assert!(output.is_empty());
    }
    for range in [
        vec![-1, 0, 1],
        vec![0, 5, 4],
        vec![0, 1],
        vec![2, 0, 1, 0],
        vec![0, 0, 1],
    ] {
        let mut doc = document();
        doc.occurrences[0].range = range;
        assert!(
            visit_index(
                &mut official_bytes(vec![doc]).as_slice(),
                &ctx,
                limit(),
                |_| Ok(())
            )
            .is_err()
        );
    }
    let mut doc = document();
    doc.occurrences[0].range.clear();
    write_document(&mut Vec::new(), &doc, &ctx, limit()).unwrap();
    doc.occurrences[0].typed_range = None;
    assert!(write_document(&mut Vec::new(), &doc, &ctx, limit()).is_err());
    let mut doc = document();
    doc.occurrences[0].enclosing_range = vec![0, 4, 4];
    assert!(write_document(&mut Vec::new(), &doc, &ctx, limit()).is_err());
    let mut doc = document();
    doc.position_encoding = protobuf::EnumOrUnknown::from_i32(99);
    assert!(write_document(&mut Vec::new(), &doc, &ctx, limit()).is_err());
}

struct ObservedRead<'a> {
    bytes: &'a [u8],
    read: Rc<Cell<usize>>,
}
impl Read for ObservedRead<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let n = self.bytes.read(buffer)?;
        self.read.set(self.read.get() + n);
        Ok(n)
    }
}
#[test]
fn corpus_exceeds_record_limit_callbacks_start_before_eof_and_duplicates_reach_staging() {
    let ctx = context();
    let mut bytes = Vec::new();
    write_metadata(&mut bytes, &metadata(), &ctx, limit()).unwrap();
    for _ in 0..1000 {
        write_document(&mut bytes, &document(), &ctx, limit()).unwrap();
    }
    assert!(bytes.len() > limit().get() as usize);
    let read = Rc::new(Cell::new(0));
    let mut input = ObservedRead {
        bytes: &bytes,
        read: Rc::clone(&read),
    };
    let mut count = 0;
    visit_index(&mut input, &ctx, limit(), |record| {
        assert!(matches!(record, IndexRecord::Document(_)));
        if count == 0 {
            assert!(read.get() < bytes.len());
        }
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 1000);
    assert_eq!(read.get(), bytes.len());
}

#[test]
fn declared_and_actual_message_bounds_apply_before_callback_and_writing() {
    let ctx = context();
    let small = NonZeroU32::new(8).unwrap();
    let bytes = official_bytes(vec![document()]);
    assert!(
        matches!(visit_index(&mut bytes.as_slice(), &ctx, small, |_| panic!("oversized callback")), Err(AstScipError::Operation(e)) if e.kind() == io::ErrorKind::FileTooLarge)
    );
    let mut declared = Vec::new();
    {
        let mut out = CodedOutputStream::vec(&mut declared);
        out.write_tag(2, WireType::LengthDelimited).unwrap();
        out.write_raw_varint64(u64::from(u32::MAX)).unwrap();
        out.flush().unwrap();
    }
    assert!(visit_index(&mut declared.as_slice(), &ctx, small, |_| Ok(())).is_err());
    let mut unknown = Vec::new();
    {
        let mut out = CodedOutputStream::vec(&mut unknown);
        out.write_tag(20, WireType::LengthDelimited).unwrap();
        out.write_raw_varint64(100).unwrap();
        out.flush().unwrap();
    }
    assert!(
        matches!(visit_index(&mut unknown.as_slice(), &ctx, small, |_| Ok(())), Err(AstScipError::Operation(e)) if e.kind() == io::ErrorKind::FileTooLarge)
    );
    let mut output = Vec::new();
    assert!(write_document(&mut output, &document(), &ctx, small).is_err());
    assert!(output.is_empty());
    // Repeated metadata fields obey the same per-message bound, not a corpus quota.
    let bounded_metadata = NonZeroU32::new(128).unwrap();
    let mut fragments = Vec::new();
    {
        let mut out = CodedOutputStream::vec(&mut fragments);
        out.write_message(1, &metadata()).unwrap();
        for _ in 0..4 {
            out.write_message(
                1,
                &Metadata {
                    tool_info: Some(ToolInfo {
                        arguments: vec!["x".repeat(48)],
                        ..Default::default()
                    })
                    .into(),
                    ..metadata()
                },
            )
            .unwrap();
        }
        out.flush().unwrap();
    }
    assert!(fragments.len() > bounded_metadata.get() as usize);
    assert!(
        visit_index(
            &mut fragments.as_slice(),
            &ctx,
            bounded_metadata,
            |_| Ok(())
        )
        .is_ok()
    );
}

#[test]
fn original_cancellation_and_expiry_block_io_and_callbacks() {
    let ctx = context();
    ctx.cancellation().cancel();
    let read = Rc::new(Cell::new(0));
    let bytes = official_bytes(vec![document()]);
    let mut input = ObservedRead {
        bytes: &bytes,
        read: Rc::clone(&read),
    };
    assert!(
        matches!(visit_index(&mut input, &ctx, limit(), |_| panic!("cancelled callback")), Err(AstScipError::Operation(e)) if e.kind() == io::ErrorKind::Interrupted)
    );
    assert_eq!(read.get(), 0);
    let mut output = Vec::new();
    assert!(write_metadata(&mut output, &metadata(), &ctx, limit()).is_err());
    assert!(output.is_empty());
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    assert!(
        matches!(visit_index(&mut bytes.as_slice(), &expired, limit(), |_| Ok(())), Err(AstScipError::Operation(e)) if e.kind() == io::ErrorKind::TimedOut)
    );
    let ctx = context();
    let mut count = 0;
    assert!(
        visit_index(&mut bytes.as_slice(), &ctx, limit(), |_| {
            count += 1;
            ctx.cancellation().cancel();
            Ok(())
        })
        .is_err()
    );
    assert_eq!(count, 1);
}

struct CancellingRead<'a> {
    bytes: &'a [u8],
    cancellation: Cancellation,
}
impl Read for CancellingRead<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let n = self.bytes.read(buffer)?;
        self.cancellation.cancel();
        Ok(n)
    }
}
struct CancellingWrite {
    cancellation: Cancellation,
    written: usize,
}
impl Write for CancellingWrite {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = usize::from(!bytes.is_empty());
        self.written += n;
        self.cancellation.cancel();
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn cancellation_during_physical_read_and_write_preserves_original_error() {
    let bytes = official_bytes(vec![document()]);
    let ctx = context();
    let mut input = CancellingRead {
        bytes: &bytes,
        cancellation: ctx.cancellation().clone(),
    };
    assert!(
        matches!(visit_index(&mut input, &ctx, limit(), |_| panic!("cancelled read")), Err(AstScipError::Operation(e)) if e.kind() == io::ErrorKind::Interrupted)
    );
    let ctx = context();
    let mut writer = CancellingWrite {
        cancellation: ctx.cancellation().clone(),
        written: 0,
    };
    assert!(
        matches!(write_document(&mut writer, &document(), &ctx, limit()), Err(AstScipError::Operation(e)) if e.kind() == io::ErrorKind::Interrupted)
    );
    assert_eq!(writer.written, 1);
}

struct FailedIo;
impl Read for FailedIo {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::ErrorKind::ConnectionReset.into())
    }
}
impl Write for FailedIo {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn physical_and_callback_errors_are_not_replaced_with_success() {
    let ctx = context();
    let error = visit_index(&mut FailedIo, &ctx, limit(), |_| Ok(())).unwrap_err();
    assert!(format!("{error:?}").contains("ConnectionReset"));
    let error = write_external_symbol(&mut FailedIo, &external(), &ctx, limit()).unwrap_err();
    assert!(format!("{error:?}").contains("BrokenPipe"));
    let bytes = official_bytes(vec![document()]);
    assert!(
        matches!(visit_index(&mut bytes.as_slice(), &ctx, limit(), |_| Err(io::Error::from(io::ErrorKind::PermissionDenied).into())), Err(AstScipError::Operation(e)) if e.kind() == io::ErrorKind::PermissionDenied)
    );
}
