use std::{fmt, io};

/// Bounded diagnostics; decoder payloads and native paths are never interpolated.
#[derive(Debug)]
pub enum DataReadError {
    InvalidPolicy(&'static str),
    InvalidManifest(&'static str),
    UnsupportedFormat(&'static str),
    InvalidParquet(&'static str),
    InvalidCsr(&'static str),
    DuplicatePrimaryKey,
    DuplicateEdgeUid,
    LimitExceeded(&'static str),
    Source(io::Error),
    Busy,
    InvalidNeighborhoodRequest,
    InvalidCursor,
    UnknownNodeLabel,
    UnknownRelationship,
    UnavailableRelationshipEndpoint,
    InvalidAnchorKey,
    AnchorNotFound,
}
impl fmt::Display for DataReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(f, "captured source: {:?}", error.kind()),
            Self::InvalidPolicy(field) => write!(f, "invalid policy: {field}"),
            Self::InvalidManifest(field) => write!(f, "invalid manifest: {field}"),
            Self::UnsupportedFormat(field) => write!(f, "unsupported format: {field}"),
            Self::InvalidParquet(field) => write!(f, "invalid parquet: {field}"),
            Self::InvalidCsr(field) => write!(f, "invalid CSR: {field}"),
            Self::LimitExceeded(field) => write!(f, "limit exceeded: {field}"),
            Self::DuplicatePrimaryKey => f.write_str("duplicate primary key"),
            Self::DuplicateEdgeUid => f.write_str("duplicate edge UID"),
            Self::InvalidNeighborhoodRequest => f.write_str("invalid neighborhood request"),
            Self::InvalidCursor => f.write_str("invalid neighborhood cursor"),
            Self::UnknownNodeLabel => f.write_str("unknown node label"),
            Self::UnknownRelationship => f.write_str("unknown relationship"),
            Self::UnavailableRelationshipEndpoint => {
                f.write_str("unavailable relationship endpoint")
            }
            Self::InvalidAnchorKey => f.write_str("invalid anchor key"),
            Self::AnchorNotFound => f.write_str("anchor not found"),
            Self::Busy => f.write_str("graph reader busy"),
        }
    }
}
impl std::error::Error for DataReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            _ => None,
        }
    }
}
impl From<io::Error> for DataReadError {
    fn from(error: io::Error) -> Self {
        Self::Source(error)
    }
}
pub(crate) type Result<T> = std::result::Result<T, DataReadError>;
pub(crate) fn add(a: usize, b: usize, field: &'static str) -> Result<usize> {
    a.checked_add(b).ok_or(DataReadError::LimitExceeded(field))
}
pub(crate) fn bounded(value: usize, maximum: usize, field: &'static str) -> Result<()> {
    if value > maximum {
        Err(DataReadError::LimitExceeded(field))
    } else {
        Ok(())
    }
}
