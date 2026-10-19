//! Shared errors and occurrence coordinates for the streaming SCIP pipeline.
use ::scip::types::{Occurrence, occurrence};
use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum AstScipError {
    Operation(io::Error),
    Protobuf(protobuf::Error),
    MissingMetadata,
    InvalidRange(&'static str),
}
impl fmt::Display for AstScipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(error) => error.fmt(f),
            Self::Protobuf(error) => error.fmt(f),
            Self::MissingMetadata => f.write_str("SCIP index lacks metadata"),
            Self::InvalidRange(reason) => write!(f, "invalid SCIP occurrence range: {reason}"),
        }
    }
}
impl Error for AstScipError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(error) => Some(error),
            Self::Protobuf(error) => Some(error),
            _ => None,
        }
    }
}
impl From<io::Error> for AstScipError {
    fn from(error: io::Error) -> Self {
        Self::Operation(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AstScipRange {
    pub start_line: u32,
    pub start_character: u32,
    pub end_line: u32,
    pub end_character: u32,
}
pub(crate) fn coordinates(occurrence: &Occurrence) -> Result<AstScipRange, AstScipError> {
    let bad = AstScipError::InvalidRange;
    let range = |a: i32, b: i32, c: i32, d: i32| {
        if a < 0 || b < 0 || c < 0 || d < 0 || (a, b) > (c, d) {
            return Err(bad("negative or reversed coordinates"));
        }
        Ok(AstScipRange {
            start_line: a as u32,
            start_character: b as u32,
            end_line: c as u32,
            end_character: d as u32,
        })
    };
    let typed = match &occurrence.typed_range {
        Some(occurrence::Typed_range::SingleLineRange(r)) => {
            Some(range(r.line, r.start_character, r.line, r.end_character)?)
        }
        Some(occurrence::Typed_range::MultiLineRange(r)) => Some(range(
            r.start_line,
            r.start_character,
            r.end_line,
            r.end_character,
        )?),
        None => None,
        Some(_) => return Err(bad("unknown typed range")),
    };
    let packed = match occurrence.range.as_slice() {
        [] if typed.is_some() => None,
        [a, b, d] => Some(range(*a, *b, *a, *d)?),
        [a, b, c, d] => Some(range(*a, *b, *c, *d)?),
        _ => {
            return Err(bad(
                "range needs three or four coordinates, or a typed range",
            ));
        }
    };
    if let (Some(a), Some(b)) = (&typed, &packed)
        && a != b
    {
        return Err(bad("typed and packed range disagree"));
    }
    typed.or(packed).ok_or_else(|| bad("missing range"))
}
