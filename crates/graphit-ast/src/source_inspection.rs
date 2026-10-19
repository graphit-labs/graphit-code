//! Source selection from indexed text or a caller-provided physical capture.
//!
//! Acquisition errors are the caller's responsibility: only an explicit indexed
//! miss permits physical reading. This module does not resolve projects, versions,
//! authorization or entity windows. A physical capture is not an authorization grant.

use graphit_core::{artifact_store::CapturedSourceReader, file_io::SnapshotContext, text_slice};
use serde::Serialize;
use std::{error::Error, fmt, io, path::Path, str::Utf8Error};

/// An acquisition outcome, including an available empty indexed document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexedSource<'s> {
    Available(&'s str),
    Missing,
}

/// A path inside the supplied capture, not a path relative to the project or cwd.
/// For a captured file, Core uses the empty path to select that file.
#[derive(Clone, Copy)]
pub struct PhysicalSource<'p> {
    pub reader: &'p CapturedSourceReader,
    pub relative: &'p Path,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceOrigin {
    Indexed,
    Physical,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct SourceInspection {
    pub origin: SourceOrigin,
    pub selection: text_slice::Result,
}

#[derive(Debug)]
pub enum SourceInspectionError {
    PhysicalUnavailable,
    Association,
    Operation(io::Error),
    PhysicalRead(io::Error),
    InvalidUtf8(Utf8Error),
    Selection(String),
    UnrepresentableLength { bytes: u64 },
}

impl fmt::Display for SourceInspectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Association => {
                formatter.write_str("Source does not match the entity graph observation")
            }
            Self::PhysicalUnavailable => formatter.write_str("physical source capture unavailable"),
            Self::Operation(error) => write!(formatter, "source operation: {error}"),
            Self::PhysicalRead(error) => write!(formatter, "physical source read: {error}"),
            Self::InvalidUtf8(error) => write!(formatter, "source is not valid UTF-8: {error}"),
            Self::Selection(error) => write!(formatter, "source selection: {error}"),
            Self::UnrepresentableLength { bytes } => {
                write!(formatter, "source length {bytes} does not fit usize")
            }
        }
    }
}

impl Error for SourceInspectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(error) | Self::PhysicalRead(error) => Some(error),
            Self::InvalidUtf8(error) => Some(error),
            Self::Association
            | Self::PhysicalUnavailable
            | Self::Selection(_)
            | Self::UnrepresentableLength { .. } => None,
        }
    }
}

/// Apply Core's line selectors without normalization or parsing.
///
/// Available indexed text never examines the physical capture. For a miss, the
/// caller must have bound that capture to the authorized local project/version
/// and this exact original request. UTF-8 is strict; BOM, CRLF and NUL are retained.
/// Core's positive line ranges clamp to the text; negative/reversed selectors fail.
/// Request checks bracket the selector, whose allocations and regex execution
/// do not provide hard preemption or recoverable allocation failure.
pub fn inspect_source(
    indexed: IndexedSource<'_>,
    physical: Option<PhysicalSource<'_>>,
    selection: &text_slice::Request,
    original: &SnapshotContext,
) -> Result<SourceInspection, SourceInspectionError> {
    inspect(indexed, physical, selection, None, original)
}

/// Apply an observed PROJECT entity extent only to bytes with its observed SHA.
/// The caller also retains and checks the actual graph/Raw association and grant.
pub fn inspect_project_entity_source(
    indexed: IndexedSource<'_>,
    physical: Option<PhysicalSource<'_>>,
    selection: &text_slice::Request,
    entity: &crate::project_entity_source::ProjectEntitySource,
    original: &SnapshotContext,
) -> Result<SourceInspection, SourceInspectionError> {
    inspect(indexed, physical, selection, Some(entity), original)
}
fn inspect(
    indexed: IndexedSource<'_>,
    physical: Option<PhysicalSource<'_>>,
    selection: &text_slice::Request,
    entity: Option<&crate::project_entity_source::ProjectEntitySource>,
    original: &SnapshotContext,
) -> Result<SourceInspection, SourceInspectionError> {
    let select = |text: &str| {
        if let Some(entity) = entity {
            if !entity.matches_text(text) {
                return Err(SourceInspectionError::Association);
            }
            crate::entity_source::select_entity_source(
                text,
                entity.start,
                entity.end,
                selection,
                original,
            )
        } else {
            text_slice::apply(text, selection).map_err(SourceInspectionError::Selection)
        }
    };
    original.check().map_err(SourceInspectionError::Operation)?;
    let (origin, selected) = match indexed {
        IndexedSource::Available(text) => (SourceOrigin::Indexed, select(text)),
        IndexedSource::Missing => {
            let physical = physical.ok_or(SourceInspectionError::PhysicalUnavailable)?;
            physical
                .reader
                .check_request(original)
                .map_err(SourceInspectionError::PhysicalRead)?;
            let file = physical
                .reader
                .open_file(physical.relative)
                .map_err(SourceInspectionError::PhysicalRead)?;
            let bytes = file.len();
            let length = usize::try_from(bytes)
                .map_err(|_| SourceInspectionError::UnrepresentableLength { bytes })?;
            let retained = file
                .read_range(0, length)
                .map_err(SourceInspectionError::PhysicalRead)?;
            let text = std::str::from_utf8(retained.as_ref())
                .map_err(SourceInspectionError::InvalidUtf8)?;
            let selected = select(text);
            physical
                .reader
                .revalidate_content()
                .map_err(SourceInspectionError::PhysicalRead)?;
            physical
                .reader
                .check_request(original)
                .map_err(SourceInspectionError::PhysicalRead)?;
            (SourceOrigin::Physical, selected)
        }
    };
    original.check().map_err(SourceInspectionError::Operation)?;
    Ok(SourceInspection {
        origin,
        selection: selected?,
    })
}
