//! Validate a supplied original span and derive its inclusive Source line window.
//!
//! Backend delimiter selection, semantic completeness, graph/content association
//! and authorization remain with callers retaining the actual acquisition owners.

use crate::{RangeError, SourceRange, SourceText};
use graphit_core::file_io::SnapshotContext;
use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum EntityExtentError {
    EmptyRange,
    ForeignRange,
    Range(RangeError),
    Operation(io::Error),
    ArithmeticOverflow,
}

impl fmt::Display for EntityExtentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRange => f.write_str("entity extent is empty"),
            Self::ForeignRange => f.write_str("entity extent does not match original source"),
            Self::Range(cause) => write!(f, "invalid entity extent: {cause}"),
            Self::Operation(cause) => write!(f, "entity extent operation failed: {cause}"),
            Self::ArithmeticOverflow => f.write_str("entity extent line arithmetic overflow"),
        }
    }
}

impl Error for EntityExtentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Range(cause) => Some(cause),
            Self::Operation(cause) => Some(cause),
            _ => None,
        }
    }
}

/// A span bound to the supplied original buffer, not proof of a complete entity.
pub struct EntityExtent<'o> {
    range: SourceRange<'o>,
    start_line: usize,
    end_line: usize,
}

impl<'o> EntityExtent<'o> {
    /// Validate an actual range before converting half-open byte points.
    ///
    /// SourceText's synchronous line indexing and request checks are cooperative.
    /// Callers retain source owners and the original operation through delivery.
    pub fn new(
        source: &SourceText<'o>,
        candidate: SourceRange<'o>,
        original: &SnapshotContext,
    ) -> Result<Self, EntityExtentError> {
        original.check().map_err(EntityExtentError::Operation)?;
        let result = (|| {
            if candidate.start() >= candidate.end() {
                return Err(EntityExtentError::EmptyRange);
            }
            let canonical = source
                .range(candidate.start(), candidate.end())
                .map_err(EntityExtentError::Range)?;
            if canonical.text().as_ptr() != candidate.text().as_ptr()
                || canonical.text().len() != candidate.text().len()
                || canonical.start() != candidate.start()
                || canonical.end() != candidate.end()
                || canonical.start_point() != candidate.start_point()
                || canonical.end_point() != candidate.end_point()
            {
                return Err(EntityExtentError::ForeignRange);
            }
            let start_line = canonical
                .start_point()
                .row
                .checked_add(1)
                .ok_or(EntityExtentError::ArithmeticOverflow)?;
            let end = canonical.end_point();
            let end_line = end
                .row
                .checked_add(usize::from(end.column != 0))
                .ok_or(EntityExtentError::ArithmeticOverflow)?;
            end_line
                .checked_sub(start_line)
                .ok_or(EntityExtentError::ArithmeticOverflow)?;
            Ok(Self {
                range: canonical,
                start_line,
                end_line,
            })
        })();
        original.check().map_err(EntityExtentError::Operation)?;
        result
    }

    pub fn range(&self) -> SourceRange<'o> {
        self.range
    }

    pub fn start_line(&self) -> usize {
        self.start_line
    }

    pub fn end_line(&self) -> usize {
        self.end_line
    }
}
