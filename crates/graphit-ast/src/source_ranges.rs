//! Borrowed source coordinates. These values carry text, not filesystem authority.

use std::{
    error::Error,
    fmt,
    sync::{Arc, OnceLock},
};

/// Zero-based row and byte column. Only LF advances a row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BytePoint {
    pub row: usize,
    pub column: usize,
}

/// A failed source admission or coordinate conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeError {
    InvalidLimit,
    InputTooLarge,
    InvalidUtf8 {
        valid_up_to: usize,
        error_len: Option<usize>,
    },
    OutOfBounds,
    NotCharBoundary,
    ReversedRange,
    InvalidPoint,
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimit => f.write_str("source byte limit must be positive"),
            Self::InputTooLarge => f.write_str("source exceeds the byte limit"),
            Self::InvalidUtf8 { .. } => f.write_str("source is not valid UTF-8"),
            Self::OutOfBounds => f.write_str("source offset is out of bounds"),
            Self::NotCharBoundary => f.write_str("source offset splits a UTF-8 character"),
            Self::ReversedRange => f.write_str("source range endpoints are reversed"),
            Self::InvalidPoint => f.write_str("source point is outside its row"),
        }
    }
}

impl Error for RangeError {}

/// A validated, unchanged view of the caller's UTF-8 bytes.
///
/// The first point conversion prepares an LF index, shared by cheap clones.
/// The original text remains borrowed and is never copied into that index.
/// The caller remains responsible for operation admission and cancellation.
#[derive(Clone)]
pub struct SourceText<'a> {
    text: &'a str,
    line_starts: Arc<OnceLock<Vec<usize>>>,
}

impl<'a> SourceText<'a> {
    pub(crate) fn as_str(&self) -> &'a str {
        self.text
    }

    /// Check the positive limit, byte length, then UTF-8 validity, in that order.
    pub fn new(bytes: &'a [u8], max_bytes: usize) -> Result<Self, RangeError> {
        if max_bytes == 0 {
            return Err(RangeError::InvalidLimit);
        }
        if bytes.len() > max_bytes {
            return Err(RangeError::InputTooLarge);
        }
        let text = std::str::from_utf8(bytes).map_err(|error| RangeError::InvalidUtf8 {
            valid_up_to: error.valid_up_to(),
            error_len: error.error_len(),
        })?;
        Ok(Self {
            text,
            line_starts: Arc::new(OnceLock::new()),
        })
    }

    pub(crate) fn line_starts(&self) -> &[usize] {
        self.line_starts.get_or_init(|| {
            std::iter::once(0)
                .chain(
                    self.text
                        .bytes()
                        .enumerate()
                        .filter_map(|(i, b)| (b == b'\n').then_some(i + 1)),
                )
                .collect()
        })
    }

    /// Convert an original byte offset, including EOF, to its byte point.
    pub fn point_at(&self, byte: usize) -> Result<BytePoint, RangeError> {
        self.check_offset(byte)?;
        let lines = self.line_starts();
        let row = lines.partition_point(|&start| start <= byte) - 1;
        Ok(BytePoint {
            row,
            column: byte - lines[row],
        })
    }

    /// Convert a point without crossing its row's LF or EOF.
    pub fn byte_at(&self, point: BytePoint) -> Result<usize, RangeError> {
        let lines = self.line_starts();
        let start = *lines.get(point.row).ok_or(RangeError::InvalidPoint)?;
        let end = lines
            .get(point.row + 1)
            .map_or(self.text.len(), |next| next - 1);
        if point.column > end - start {
            return Err(RangeError::InvalidPoint);
        }
        // The admitted column is within this row, so addition cannot overflow.
        let byte = start + point.column;
        self.check_offset(byte)?;
        Ok(byte)
    }

    /// Convert an original Unicode scalar ordinal, including the EOF ordinal.
    ///
    /// This is not an ANTLR runtime index: a runtime may remove a leading BOM.
    pub fn scalar_byte(&self, index: usize) -> Result<usize, RangeError> {
        self.text
            .char_indices()
            .map(|(byte, _)| byte)
            .chain(std::iter::once(self.text.len()))
            .nth(index)
            .ok_or(RangeError::OutOfBounds)
    }

    /// Borrow a half-open range, checking reversal before bounds and boundaries.
    ///
    /// The range borrows the original bytes, not this wrapper.
    ///
    /// ```
    /// use graphit_ast::SourceText;
    /// let bytes = String::from("é\n");
    /// let range = {
    ///     let view = SourceText::new(bytes.as_bytes(), 3)?;
    ///     view.range(0, 2)?
    /// };
    /// assert_eq!(range.text(), "é");
    /// # Ok::<(), graphit_ast::RangeError>(())
    /// ```
    ///
    /// ```compile_fail
    /// use graphit_ast::SourceText;
    /// let mut bytes = String::from("code");
    /// let view = SourceText::new(bytes.as_bytes(), 4).unwrap();
    /// let range = view.range(0, 4).unwrap();
    /// bytes.clear();
    /// assert_eq!(range.text(), "code");
    /// ```
    pub fn range(&self, start: usize, end: usize) -> Result<SourceRange<'a>, RangeError> {
        if start > end {
            return Err(RangeError::ReversedRange);
        }
        if end > self.text.len() {
            return Err(RangeError::OutOfBounds);
        }
        self.check_offset(start)?;
        self.check_offset(end)?;
        let text = self
            .text
            .get(start..end)
            .ok_or(RangeError::NotCharBoundary)?;
        Ok(SourceRange {
            text,
            start,
            end,
            start_point: self.point_at(start)?,
            end_point: self.point_at(end)?,
        })
    }

    fn check_offset(&self, byte: usize) -> Result<(), RangeError> {
        if byte > self.text.len() {
            return Err(RangeError::OutOfBounds);
        }
        if !self.text.is_char_boundary(byte) {
            return Err(RangeError::NotCharBoundary);
        }
        Ok(())
    }
}

/// A subslice with validated offsets and points in the complete original source.
#[derive(Clone, Copy)]
pub struct SourceRange<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    start_point: BytePoint,
    end_point: BytePoint,
}

impl<'a> SourceRange<'a> {
    pub fn text(&self) -> &'a str {
        self.text
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn start_point(&self) -> BytePoint {
        self.start_point
    }

    pub fn end_point(&self) -> BytePoint {
        self.end_point
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_points_preserve_original_coordinates_and_ranges() {
        for original in [
            "",
            "ascii",
            "\u{feff}é中🦀e\u{0301}",
            "é\r\n🦀\n",
            "\n\n",
            "a\rb",
        ] {
            let source = SourceText::new(original.as_bytes(), original.len().max(1)).unwrap();
            for byte in 0..=original.len() {
                if !original.is_char_boundary(byte) {
                    assert_eq!(source.point_at(byte), Err(RangeError::NotCharBoundary));
                    continue;
                }
                // Independent prefix scan is the previous coordinate contract.
                let prefix = &original.as_bytes()[..byte];
                let expected = BytePoint {
                    row: prefix.iter().filter(|&&b| b == b'\n').count(),
                    column: prefix
                        .iter()
                        .rposition(|&b| b == b'\n')
                        .map_or(byte, |newline| byte - newline - 1),
                };
                assert_eq!(source.point_at(byte), Ok(expected));
                assert_eq!(source.byte_at(expected), Ok(byte));
                let range = source.range(byte, original.len()).unwrap();
                assert_eq!(range.start_point(), expected);
                assert_eq!(range.text().as_ptr(), original[byte..].as_ptr());
                assert_eq!(range.text(), &original[byte..]);
            }
        }
    }

    #[test]
    fn clones_share_one_lazy_line_index_and_ranges_outlive_views() {
        let original = String::from("a\né\n");
        let source = SourceText::new(original.as_bytes(), original.len()).unwrap();
        let clone = source.clone();
        assert!(Arc::ptr_eq(&source.line_starts, &clone.line_starts));
        assert!(source.line_starts.get().is_none());
        assert_eq!(source.point_at(2), Ok(BytePoint { row: 1, column: 0 }));
        assert_eq!(source.line_starts(), [0, 2, 5]);
        assert!(std::ptr::eq(source.line_starts(), clone.line_starts()));
        drop(source);
        let range = clone.range(2, 4).unwrap();
        drop(clone);
        assert_eq!(range.text(), "é");
        assert_eq!(range.text().as_ptr(), original[2..4].as_ptr());
    }
}
