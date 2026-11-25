//! Pure Markdown previews over caller-provided UTF-8 text and request context.
use graphit_core::file_io::SnapshotContext;
use std::{
    borrow::Cow, collections::TryReserveError, error::Error, fmt, io, num::NonZeroUsize, ops::Range,
};

/// Explicit hard byte limits and a soft Unicode scalar limit for the body.
#[derive(Debug, Copy, Clone)]
pub struct MarkdownPreviewLimits {
    pub max_input_bytes: NonZeroUsize,
    pub max_output_bytes: NonZeroUsize,
    pub max_body_scalars: NonZeroUsize,
}

/// Text selection only; this does not establish physical provenance or access.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PreviewSource {
    Summary,
    Body,
}

/// Selected text and its exact range in the selected original input.
///
/// The range excludes the synthetic ellipsis. Uncut text borrows its input;
/// clipped text owns only its prefix and marker. Neither branch borrows context.
#[derive(Debug)]
pub struct MarkdownPreview<'a> {
    text: Cow<'a, str>,
    source: PreviewSource,
    range: Range<usize>,
    truncated: bool,
}

impl MarkdownPreview<'_> {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn source(&self) -> PreviewSource {
        self.source
    }

    pub fn source_range(&self) -> Range<usize> {
        self.range.clone()
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Debug)]
pub enum MarkdownPreviewError {
    Context(io::Error),
    LimitExceeded(&'static str),
    Allocation(TryReserveError),
}

impl MarkdownPreviewError {
    pub fn kind(&self) -> io::ErrorKind {
        match self {
            Self::Context(error) => error.kind(),
            Self::LimitExceeded(_) => io::ErrorKind::FileTooLarge,
            Self::Allocation(_) => io::ErrorKind::OutOfMemory,
        }
    }
}

impl fmt::Display for MarkdownPreviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Context(_) => "Markdown preview request is unavailable",
            Self::LimitExceeded(_) => "Markdown preview exceeds a limit",
            Self::Allocation(_) => "Markdown preview allocation failed",
        })
    }
}

impl Error for MarkdownPreviewError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Context(error) => Some(error),
            Self::Allocation(error) => Some(error),
            Self::LimitExceeded(_) => None,
        }
    }
}

fn check(context: &SnapshotContext) -> Result<(), MarkdownPreviewError> {
    context.check().map_err(MarkdownPreviewError::Context)
}

fn admit_output(bytes: usize, limit: NonZeroUsize) -> Result<(), MarkdownPreviewError> {
    if bytes > limit.get() {
        return Err(MarkdownPreviewError::LimitExceeded("output bytes"));
    }
    Ok(())
}

fn whole<'a>(
    context: &SnapshotContext,
    text: &'a str,
    source: PreviewSource,
    limit: NonZeroUsize,
) -> Result<MarkdownPreview<'a>, MarkdownPreviewError> {
    admit_output(text.len(), limit)?;
    check(context)?;
    Ok(MarkdownPreview {
        text: Cow::Borrowed(text),
        source,
        range: 0..text.len(),
        truncated: false,
    })
}

/// Prefer a non-whitespace summary in full; otherwise preview the raw body.
///
/// Both inputs count toward the hard input limit. The body scalar cap admits
/// one lookahead scalar, then prefers the last whitespace delimiter in the
/// latter half of the prefix. It is not a grapheme boundary or Markdown parser.
/// The complete selected output, including a clipped prefix's three-byte
/// ellipsis, must fit the hard output limit. Limit failures never fall back.
/// Checks are cooperative and perform no I/O or request accounting mutations.
pub fn preview_markdown<'a>(
    context: &SnapshotContext,
    body: &'a str,
    summary: &'a str,
    limits: MarkdownPreviewLimits,
) -> Result<MarkdownPreview<'a>, MarkdownPreviewError> {
    check(context)?;
    let input_bytes = body
        .len()
        .checked_add(summary.len())
        .ok_or(MarkdownPreviewError::LimitExceeded("input bytes"))?;
    if input_bytes > limits.max_input_bytes.get() {
        return Err(MarkdownPreviewError::LimitExceeded("input bytes"));
    }

    for scalar in summary.chars() {
        check(context)?;
        if !scalar.is_whitespace() {
            return whole(
                context,
                summary,
                PreviewSource::Summary,
                limits.max_output_bytes,
            );
        }
    }

    let cap = limits.max_body_scalars.get();
    let mut count = 0usize;
    let mut boundary = 0usize;
    let mut delimiter = None;
    let mut clipped = false;
    for (index, scalar) in body.char_indices() {
        check(context)?;
        if count == cap {
            clipped = true;
            break;
        }
        if count >= cap / 2 && index > 0 && scalar.is_whitespace() {
            delimiter = Some(index);
        }
        boundary = index
            .checked_add(scalar.len_utf8())
            .ok_or(MarkdownPreviewError::LimitExceeded("input bytes"))?;
        count = count
            .checked_add(1)
            .ok_or(MarkdownPreviewError::LimitExceeded("input bytes"))?;
    }
    if !clipped {
        return whole(context, body, PreviewSource::Body, limits.max_output_bytes);
    }

    const MARKER: &str = "…";
    let cut = delimiter.unwrap_or(boundary);
    let output_bytes = cut
        .checked_add(MARKER.len())
        .ok_or(MarkdownPreviewError::LimitExceeded("output bytes"))?;
    admit_output(output_bytes, limits.max_output_bytes)?;
    check(context)?;
    let mut text = String::new();
    text.try_reserve_exact(output_bytes)
        .map_err(MarkdownPreviewError::Allocation)?;
    check(context)?;
    text.push_str(&body[..cut]);
    check(context)?;
    text.push_str(MARKER);
    check(context)?;
    Ok(MarkdownPreview {
        text: Cow::Owned(text),
        source: PreviewSource::Body,
        range: 0..cut,
        truncated: true,
    })
}
