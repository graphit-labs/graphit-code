//! Metadata for deliberate Knowledge text exports.
use chrono::{DateTime, SecondsFormat, Utc};
use graphit_core::file_io::SnapshotContext;
use graphit_data::knowledge_index::KnowledgeDocument;
use serde::Serialize;
use std::{
    error::Error,
    fmt, io,
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
pub enum ExportMetadataError {
    Parse(Box<crate::document::DocumentParseError>),
    Request(io::Error),
    Serialization(serde_saphyr::SerializeError),
    OutOfRange(&'static str),
}
impl From<crate::document::DocumentParseError> for ExportMetadataError {
    fn from(error: crate::document::DocumentParseError) -> Self {
        match error {
            crate::document::DocumentParseError::Request(error) => Self::Request(error),
            error => Self::Parse(Box::new(error)),
        }
    }
}
impl fmt::Display for ExportMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(_) => f.write_str("Knowledge source navigation metadata invalid"),
            Self::Request(_) => f.write_str("Knowledge source request failed"),
            Self::Serialization(_) => f.write_str("Knowledge source YAML encoding failed"),
            Self::OutOfRange(label) => write!(f, "Knowledge source cannot represent {label}"),
        }
    }
}
impl Error for ExportMetadataError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error.as_ref()),
            Self::Request(e) => Some(e),
            Self::Serialization(e) => Some(e),
            _ => None,
        }
    }
}
type Result<T> = std::result::Result<T, ExportMetadataError>;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    previous: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next: Option<String>,
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
    .map_err(|_| ExportMetadataError::OutOfRange(label))?;
    let nanos = i64::try_from(nanos).map_err(|_| ExportMetadataError::OutOfRange(label))?;
    Ok(DateTime::<Utc>::from_timestamp_nanos(nanos).to_rfc3339_opts(SecondsFormat::Nanos, true))
}

/// Build the existing native metadata without exposing a parallel public DTO.
pub(crate) fn native_metadata<'a>(
    original: &SnapshotContext,
    document: &'a KnowledgeDocument,
) -> Result<impl Serialize + 'a> {
    original.check().map_err(ExportMetadataError::Request)?;
    let navigation = crate::document::document_navigation(original, &document.body)
        .map_err(ExportMetadataError::from)?;
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
                .map_err(|_| ExportMetadataError::OutOfRange("summary bytes"))?,
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
                    .map_err(|_| ExportMetadataError::OutOfRange("stale_after_ns"))
            })
            .transpose()?,
        resolved_wiki_targets: &document.resolved_wiki_targets,
        previous: navigation.previous,
        next: navigation.next,
    };
    Ok(header)
}

/// Serialize native metadata; framing and bounded writer stay with the caller.
pub(crate) fn write_native_metadata(
    original: &SnapshotContext,
    header: &impl Serialize,
    writer: &mut impl Write,
) -> Result<()> {
    original.check().map_err(ExportMetadataError::Request)?;
    let options = serde_saphyr::ser_options! { quote_all: true, prefer_block_scalars: false, yaml_12: false, tagged_enums: false };
    serde_saphyr::to_io_writer_with_options(writer, header, options)
        .map_err(ExportMetadataError::Serialization)?;
    original.check().map_err(ExportMetadataError::Request)
}
