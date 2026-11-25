//! Pure selection of an entity's line window from caller-retained raw Source.
//!
//! Window coordinates are selector inputs, not entity lookup, content binding or
//! authorization. Callers retain those owners through selection and final
//! authorization checks before delivery.

use crate::source_inspection::SourceInspectionError;
use graphit_core::{file_io::SnapshotContext, text_slice};

/// Select raw UTF-8 Source using Core's strict 1-based inclusive window.
///
/// Request ranges are relative to the window. Result coordinates and total lines
/// describe the original full file. The original operation is checked before and
/// after Core selection; regex execution and allocation remain cooperative.
///
/// This function neither acquires Source nor authenticates a caller. A route must
/// establish entity/content binding and retain its real graph, Source and request
/// owners through selection and final authorization.
pub fn select_entity_source(
    text: &str,
    window_start: usize,
    window_end: usize,
    selection: &text_slice::Request,
    original: &SnapshotContext,
) -> Result<text_slice::Result, SourceInspectionError> {
    original.check().map_err(SourceInspectionError::Operation)?;
    let result = text_slice::apply_window(text, selection, window_start, window_end);
    original.check().map_err(SourceInspectionError::Operation)?;
    result.map_err(SourceInspectionError::Selection)
}
