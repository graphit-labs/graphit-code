//! Native virtual source for an already-read complete Knowledge document.
use chrono::{DateTime, SecondsFormat, Utc};
use graphit_core::{file_io::SnapshotContext, text_slice};
use graphit_data::knowledge_index::KnowledgeDocument;
use serde::Serialize;
use std::{
    collections::TryReserveError,
    error::Error,
    fmt, io,
    io::Write,
    num::NonZeroUsize,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug)]
pub struct DocumentSourceLimits {
    /// Aggregate body, textual metadata, target and pattern bytes.
    pub max_input_bytes: NonZeroUsize,
    pub max_targets: NonZeroUsize,
    /// Complete virtual source, including YAML escapes and framing.
    pub max_virtual_bytes: NonZeroUsize,
    /// Includes the final empty line when the source ends with LF.
    pub max_virtual_lines: NonZeroUsize,
    /// Returned source plus all copied Match.line bytes, after Core selection.
    pub max_output_bytes: NonZeroUsize,
    /// All returned matches, including context lines.
    pub max_matches: NonZeroUsize,
}

#[derive(Debug)]
pub enum DocumentSourceError {
    Request(io::Error),
    Serialization(serde_saphyr::SerializeError),
    Selection(String),
    Limit(&'static str),
    Overflow(&'static str),
    OutOfRange(&'static str),
    Allocation(TryReserveError),
    Encoding(std::string::FromUtf8Error),
}
impl fmt::Display for DocumentSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(_) => f.write_str("Knowledge source request failed"),
            Self::Serialization(_) => f.write_str("Knowledge source YAML encoding failed"),
            Self::Selection(e) => write!(f, "Knowledge source selection: {e}"),
            Self::Limit(label) => write!(f, "Knowledge source exceeded {label}"),
            Self::Overflow(label) => write!(f, "Knowledge source overflowed {label}"),
            Self::OutOfRange(label) => write!(f, "Knowledge source cannot represent {label}"),
            Self::Allocation(_) => f.write_str("Knowledge source buffer allocation failed"),
            Self::Encoding(_) => f.write_str("Knowledge source encoding is not UTF-8"),
        }
    }
}
impl Error for DocumentSourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Serialization(e) => Some(e),
            Self::Allocation(e) => Some(e),
            Self::Encoding(e) => Some(e),
            _ => None,
        }
    }
}
type Result<T> = std::result::Result<T, DocumentSourceError>;

#[derive(Serialize)]
struct Header<'a> {
    id: &'a str,
    title: &'a str,
    #[serde(rename = "type")]
    kind: &'a str,
    summary: &'a str,
    description: Option<&'a str>,
    summary_truncated: bool,
    source_path: &'a str,
    physical_digest: Option<String>,
    compilation_digest: String,
    recipe: &'a str,
    recipe_parameters: Recipe<'a>,
    updated_at: Option<String>,
    stale_since: Option<String>,
    stale_after_ns: Option<u64>,
    resolved_wiki_targets: &'a [String],
}
#[derive(Serialize)]
struct Recipe<'a> {
    fallback_title: &'a str,
    kind_on_absent: &'a str,
    max_summary_bytes: u64,
}

fn timestamp(time: SystemTime, label: &'static str) -> Result<String> {
    let nanos = match time.duration_since(UNIX_EPOCH) {
        Ok(d) => i128::try_from(d.as_nanos()),
        Err(e) => i128::try_from(e.duration().as_nanos()).map(|n| -n),
    }
    .map_err(|_| DocumentSourceError::OutOfRange(label))?;
    let nanos = i64::try_from(nanos).map_err(|_| DocumentSourceError::OutOfRange(label))?;
    Ok(DateTime::<Utc>::from_timestamp_nanos(nanos).to_rfc3339_opts(SecondsFormat::Nanos, true))
}

fn added(current: usize, extra: usize, label: &'static str) -> Result<usize> {
    current
        .checked_add(extra)
        .ok_or(DocumentSourceError::Overflow(label))
}
fn capped(used: usize, limit: NonZeroUsize, label: &'static str) -> Result<()> {
    if used > limit.get() {
        return Err(DocumentSourceError::Limit(label));
    }
    Ok(())
}
fn input_size(original: &SnapshotContext, d: &KnowledgeDocument, pattern: &str) -> Result<usize> {
    let mut size = 0;
    for text in [
        &d.slug,
        &d.title,
        &d.body,
        &d.kind,
        &d.summary,
        d.description.as_deref().unwrap_or(""),
        &d.source_path,
        &d.recipe,
        &d.recipe_parameters.fallback_title,
        &d.recipe_parameters.kind_on_absent,
        pattern,
    ]
    .into_iter()
    .chain(d.resolved_wiki_targets.iter().map(String::as_str))
    {
        original.check().map_err(DocumentSourceError::Request)?;
        size = added(size, text.len(), "input bytes")?;
    }
    Ok(size)
}

/// Build the existing native metadata without exposing a parallel public DTO.
pub(crate) fn native_metadata<'a>(
    original: &SnapshotContext,
    document: &'a KnowledgeDocument,
) -> Result<impl Serialize + 'a> {
    original.check().map_err(DocumentSourceError::Request)?;
    let header = Header {
        id: &document.slug,
        title: &document.title,
        kind: &document.kind,
        summary: &document.summary,
        description: document.description.as_deref(),
        summary_truncated: document.summary_truncated,
        source_path: &document.source_path,
        physical_digest: document.physical_digest.map(|d| hex::encode(d.as_bytes())),
        compilation_digest: hex::encode(document.compilation_digest.as_bytes()),
        recipe: &document.recipe,
        recipe_parameters: Recipe {
            fallback_title: &document.recipe_parameters.fallback_title,
            kind_on_absent: &document.recipe_parameters.kind_on_absent,
            max_summary_bytes: u64::try_from(document.recipe_parameters.max_summary_bytes.get())
                .map_err(|_| DocumentSourceError::OutOfRange("summary bytes"))?,
        },
        updated_at: document
            .updated_at
            .map(|t| timestamp(t, "updated_at"))
            .transpose()?,
        stale_since: document
            .stale_since
            .map(|t| timestamp(t, "stale_since"))
            .transpose()?,
        stale_after_ns: document
            .stale_after
            .map(|d| {
                u64::try_from(d.as_nanos())
                    .map_err(|_| DocumentSourceError::OutOfRange("stale_after_ns"))
            })
            .transpose()?,
        resolved_wiki_targets: &document.resolved_wiki_targets,
    };
    Ok(header)
}

/// Serialize native metadata; framing and bounded writer stay with the caller.
pub(crate) fn write_native_metadata(
    original: &SnapshotContext,
    header: &impl Serialize,
    writer: &mut impl Write,
) -> Result<()> {
    original.check().map_err(DocumentSourceError::Request)?;
    let options = serde_saphyr::ser_options! { quote_all: true, prefer_block_scalars: false, yaml_12: false, tagged_enums: false };
    serde_saphyr::to_io_writer_with_options(writer, header, options)
        .map_err(DocumentSourceError::Serialization)?;
    original.check().map_err(DocumentSourceError::Request)
}

/// Render native metadata and the literal complete body, then use Core selection.
///
/// The caller owns lookup, authorized Data reading and generation. This function
/// neither admits access nor pins storage. Checks are cooperative; SDK/Core and
/// their allocations are not preemptible. Output limits apply after selection.
pub fn render_document_source(
    original: &SnapshotContext,
    document: &KnowledgeDocument,
    selection: &text_slice::Request,
    limits: DocumentSourceLimits,
) -> Result<text_slice::Result> {
    original.check().map_err(DocumentSourceError::Request)?;
    capped(
        document.resolved_wiki_targets.len(),
        limits.max_targets,
        "targets",
    )?;
    capped(
        input_size(original, document, &selection.pattern)?,
        limits.max_input_bytes,
        "input bytes",
    )?;
    let header = native_metadata(original, document)?;
    let mut writer = SourceWriter {
        original,
        bytes: Vec::new(),
        limit: limits.max_virtual_bytes,
        failure: None,
    };
    writer.append(b"---\n")?;
    if let Err(e) = write_native_metadata(original, &header, &mut writer) {
        return Err(writer.failure.take().unwrap_or(e));
    }
    if !writer.bytes.ends_with(b"\n") {
        writer.append(b"\n")?;
    }
    writer.append(b"---\n\n")?;
    writer.append(document.body.as_bytes())?;
    let lines = added(
        writer.bytes.iter().filter(|b| **b == b'\n').count(),
        1,
        "virtual lines",
    )?;
    capped(lines, limits.max_virtual_lines, "virtual lines")?;
    let source = String::from_utf8(writer.bytes).map_err(DocumentSourceError::Encoding)?;
    original.check().map_err(DocumentSourceError::Request)?;
    let selected = text_slice::apply(&source, selection).map_err(DocumentSourceError::Selection)?;
    original.check().map_err(DocumentSourceError::Request)?;
    capped(selected.matches.len(), limits.max_matches, "matches")?;
    let mut output = selected.source.len();
    for row in &selected.matches {
        original.check().map_err(DocumentSourceError::Request)?;
        output = added(output, row.line.len(), "output bytes")?;
    }
    capped(output, limits.max_output_bytes, "output bytes")?;
    original.check().map_err(DocumentSourceError::Request)?;
    Ok(selected)
}

struct SourceWriter<'a> {
    original: &'a SnapshotContext,
    bytes: Vec<u8>,
    limit: NonZeroUsize,
    failure: Option<DocumentSourceError>,
}
impl SourceWriter<'_> {
    fn append(&mut self, bytes: &[u8]) -> Result<()> {
        self.original
            .check()
            .map_err(DocumentSourceError::Request)?;
        let size = added(self.bytes.len(), bytes.len(), "virtual bytes")?;
        capped(size, self.limit, "virtual bytes")?;
        self.bytes
            .try_reserve(bytes.len())
            .map_err(DocumentSourceError::Allocation)?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}
impl Write for SourceWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.write_all(bytes).map(|()| bytes.len())
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.failure.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        if let Err(e) = self.append(bytes) {
            let kind = match &e {
                DocumentSourceError::Request(e) => e.kind(),
                DocumentSourceError::Limit(_) => io::ErrorKind::FileTooLarge,
                _ => io::ErrorKind::Other,
            };
            self.failure = Some(e);
            return Err(kind.into());
        }
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.write_all(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::{Duration, Instant};

    #[test]
    fn overflow_is_a_distinct_error() {
        assert!(matches!(
            added(usize::MAX, 1, "virtual bytes"),
            Err(DocumentSourceError::Overflow("virtual bytes"))
        ));
    }

    #[test]
    fn sdk_writer_never_retries_interrupted_and_preserves_first_failure() {
        let cancel = Cancellation::default();
        let original =
            SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
        let mut writer = SourceWriter {
            original: &original,
            bytes: Vec::new(),
            limit: NonZeroUsize::new(100).unwrap(),
            failure: None,
        };
        writer.write_all(b"retained prefix").unwrap();
        cancel.cancel();
        let sdk_error = serde_saphyr::to_io_writer(&mut writer, &"rejected").unwrap_err();
        assert!(sdk_error.source().is_some());
        assert!(
            matches!(writer.failure.as_ref(), Some(DocumentSourceError::Request(e)) if e.kind() == io::ErrorKind::Interrupted)
        );
        assert_eq!(writer.bytes, b"retained prefix");
        assert!(writer.write_all(&[]).is_err());
        assert!(writer.flush().is_err());
        assert!(
            matches!(writer.failure, Some(DocumentSourceError::Request(e)) if e.kind() == io::ErrorKind::Interrupted)
        );
    }
}
