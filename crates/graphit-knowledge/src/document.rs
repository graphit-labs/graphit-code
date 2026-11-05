//! Source-preserving document syntax, without storage, resolution or I/O.
//!
//! Limits count admitted bytes, emitted events and cumulative retained text.
//! They do not preempt a synchronous SDK call or measure allocator RSS.

use graphit_core::file_io::SnapshotContext;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde_saphyr::granit_parser::{self as yaml, ScalarStyle};
use std::{
    borrow::Cow, collections::HashSet, error::Error, fmt, io, num::NonZeroUsize, ops::Range,
    str::Utf8Error,
};

#[derive(Clone, Copy, Debug)]
pub struct DocumentParseLimits {
    pub max_document_bytes: NonZeroUsize,
    pub max_input_bytes: NonZeroUsize,
    pub max_frontmatter_bytes: NonZeroUsize,
    pub max_yaml_events: NonZeroUsize,
    pub max_markdown_events: NonZeroUsize,
    pub max_yaml_depth: NonZeroUsize,
    pub max_scalar_bytes: NonZeroUsize,
    pub max_links: NonZeroUsize,
    pub max_projection_bytes: NonZeroUsize,
    pub max_summary_bytes: NonZeroUsize,
}

#[derive(Debug)]
pub struct ParsedDocument<'a> {
    pub source: &'a str,
    pub envelope: DocumentEnvelope,
    pub metadata: Metadata<'a>,
    pub markdown: Vec<MarkdownEvent<'a>>,
    pub title: String,
    pub summary: String,
    pub summary_truncated: bool,
    pub stats: ParseStats,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentEnvelope {
    pub bom: Option<Range<usize>>,
    /// Delimiters and their line endings, excluding the BOM.
    pub frontmatter: Option<Range<usize>>,
    pub yaml: Option<Range<usize>>,
    pub body: Range<usize>,
}

#[derive(Debug)]
pub struct MarkdownEvent<'a> {
    pub event: Event<'a>,
    pub span: Range<usize>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NodeId(usize);

impl NodeId {
    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Default)]
pub struct Metadata<'a> {
    pub root: Option<NodeId>,
    nodes: Vec<MetadataNode<'a>>,
}

impl<'a> Metadata<'a> {
    pub fn nodes(&self) -> &[MetadataNode<'a>] {
        &self.nodes
    }

    pub fn node(&self, id: NodeId) -> Option<&MetadataNode<'a>> {
        self.nodes.get(id.0)
    }
}

#[derive(Debug)]
pub struct MetadataNode<'a> {
    /// Original UTF-8 byte range; collection spans include matching closings.
    pub span: Range<usize>,
    pub kind: MetadataKind<'a>,
}

#[derive(Debug)]
pub enum MetadataKind<'a> {
    /// An omitted value may be emitted as `~` with a delimiter or empty span.
    /// This is syntax, without YAML schema coercion or a synthetic-value flag.
    Scalar {
        value: Cow<'a, str>,
        style: ScalarStyle,
    },
    Sequence {
        items: Vec<NodeId>,
    },
    Mapping {
        entries: Vec<(NodeId, NodeId)>,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParseStats {
    pub yaml_events: usize,
    pub markdown_events: usize,
    pub yaml_depth: usize,
    pub links: usize,
    pub projection_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct YamlLocation {
    /// One-based line within the YAML payload.
    pub yaml_line: usize,
    /// Zero-based Unicode character column, not a byte offset.
    pub yaml_column: usize,
    /// One-based line within the entire original document.
    pub document_line: usize,
}

#[derive(Debug)]
pub enum DocumentParseError {
    LimitExceeded(&'static str),
    InvalidUtf8(Utf8Error),
    UnclosedFrontmatter,
    Yaml {
        source: yaml::ScanError,
        location: YamlLocation,
    },
    InvalidFieldType(&'static str),
    DuplicateKey,
    UnsupportedFeature(&'static str),
    MultipleDocuments,
    InvalidSpan,
    Request(io::Error),
}

impl fmt::Display for DocumentParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded(name) => write!(f, "document parse limit exceeded: {name}"),
            Self::InvalidUtf8(_) => f.write_str("document is not valid UTF-8"),
            Self::UnclosedFrontmatter => f.write_str("frontmatter has no closing delimiter"),
            Self::Yaml { location, .. } => write!(
                f,
                "invalid YAML at document line {}, column {}",
                location.document_line, location.yaml_column
            ),
            Self::InvalidFieldType(name) => write!(f, "invalid document field type: {name}"),
            Self::DuplicateKey => f.write_str("duplicate YAML mapping key"),
            Self::UnsupportedFeature(name) => write!(f, "unsupported document syntax: {name}"),
            Self::MultipleDocuments => f.write_str("frontmatter contains multiple YAML documents"),
            Self::InvalidSpan => f.write_str("invalid document syntax span"),
            Self::Request(_) => f.write_str("document request is unavailable"),
        }
    }
}

impl Error for DocumentParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidUtf8(source) => Some(source),
            Self::Yaml { source, .. } => Some(source),
            Self::Request(source) => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for DocumentParseError {
    fn from(error: io::Error) -> Self {
        Self::Request(error)
    }
}

type Result<T> = std::result::Result<T, DocumentParseError>;

/// Parse bytes captured by the caller under its own authority.
///
/// The supplied context is checked throughout, without reserving any I/O or
/// creating a request. Bytes alone cannot establish their capture identity.
pub fn parse_document<'a>(
    context: &SnapshotContext,
    input: &'a [u8],
    fallback_title: &str,
    limits: DocumentParseLimits,
) -> std::result::Result<ParsedDocument<'a>, DocumentParseError> {
    context.check()?;
    admit(0, input.len(), limits.max_document_bytes, "document bytes")?;
    admit(
        input.len(),
        fallback_title.len(),
        limits.max_input_bytes,
        "input bytes",
    )?;
    admit(
        0,
        fallback_title.len(),
        limits.max_scalar_bytes,
        "scalar bytes",
    )?;
    let source = std::str::from_utf8(input).map_err(DocumentParseError::InvalidUtf8)?;
    let envelope = envelope(context, source, limits.max_frontmatter_bytes)?;
    let mut budget = TextBudget {
        context,
        limit: limits.max_projection_bytes,
        used: 0,
    };
    let mut stats = ParseStats::default();
    let metadata = match &envelope.yaml {
        Some(range) => parse_yaml(
            context,
            source,
            range.clone(),
            limits,
            &mut budget,
            &mut stats,
        )?,
        None => Metadata::default(),
    };
    let explicit_title = editorial_field(context, &metadata, "title")?;
    let explicit_summary = editorial_field(context, &metadata, "description")?;
    let (markdown, editorial) = parse_markdown(
        context,
        source,
        envelope.body.clone(),
        limits,
        &mut budget,
        &mut stats,
    )?;
    let title = budget.copy(
        explicit_title
            .or(editorial.title.as_deref())
            .unwrap_or(fallback_title)
            .trim(),
    )?;
    let selected_summary = explicit_summary
        .or(editorial.summary.as_deref())
        .unwrap_or("")
        .trim();
    let end = prefix_boundary(selected_summary, limits.max_summary_bytes.get());
    let summary = budget.copy(&selected_summary[..end])?;
    let summary_truncated = end < selected_summary.len();
    stats.projection_bytes = budget.used;
    context.check()?;
    Ok(ParsedDocument {
        source,
        envelope,
        metadata,
        markdown,
        title,
        summary,
        summary_truncated,
        stats,
    })
}

fn admit(current: usize, amount: usize, limit: NonZeroUsize, name: &'static str) -> Result<usize> {
    current
        .checked_add(amount)
        .filter(|total| *total <= limit.get())
        .ok_or(DocumentParseError::LimitExceeded(name))
}

fn reserve<T>(values: &mut Vec<T>) -> Result<()> {
    values
        .try_reserve(1)
        .map_err(|_| DocumentParseError::Request(io::ErrorKind::OutOfMemory.into()))
}

struct TextBudget<'a> {
    context: &'a SnapshotContext,
    limit: NonZeroUsize,
    used: usize,
}

impl TextBudget<'_> {
    fn charge(&mut self, amount: usize) -> Result<()> {
        self.context.check()?;
        self.used = admit(self.used, amount, self.limit, "projection bytes")?;
        Ok(())
    }

    fn append(&mut self, output: &mut String, text: &str) -> Result<()> {
        self.charge(text.len())?;
        output
            .try_reserve(text.len())
            .map_err(|_| DocumentParseError::Request(io::ErrorKind::OutOfMemory.into()))?;
        output.push_str(text);
        Ok(())
    }

    fn copy(&mut self, text: &str) -> Result<String> {
        let mut output = String::new();
        self.append(&mut output, text)?;
        Ok(output)
    }
}

fn source_span(
    source: &str,
    local: Range<usize>,
    base: usize,
    local_len: usize,
) -> Result<Range<usize>> {
    if local.start > local.end || local.end > local_len {
        return Err(DocumentParseError::InvalidSpan);
    }
    let start = base
        .checked_add(local.start)
        .ok_or(DocumentParseError::InvalidSpan)?;
    let end = base
        .checked_add(local.end)
        .ok_or(DocumentParseError::InvalidSpan)?;
    if source.get(start..end).is_none() {
        return Err(DocumentParseError::InvalidSpan);
    }
    Ok(start..end)
}

pub(crate) fn envelope(
    context: &SnapshotContext,
    source: &str,
    max_bytes: NonZeroUsize,
) -> Result<DocumentEnvelope> {
    let bom = source.starts_with('\u{feff}').then_some(0..3);
    let start = bom.as_ref().map_or(0, |range| range.end);
    let mut lines = source[start..].split_inclusive('\n');
    let Some(first) = lines.next() else {
        return Ok(DocumentEnvelope {
            bom,
            frontmatter: None,
            yaml: None,
            body: start..source.len(),
        });
    };
    context.check()?;
    if line_content(first) != "---" {
        return Ok(DocumentEnvelope {
            bom,
            frontmatter: None,
            yaml: None,
            body: start..source.len(),
        });
    }
    let yaml_start = start
        .checked_add(first.len())
        .ok_or(DocumentParseError::InvalidSpan)?;
    admit(0, first.len(), max_bytes, "frontmatter bytes")?;
    let mut position = yaml_start;
    for line in lines {
        context.check()?;
        let end = position
            .checked_add(line.len())
            .ok_or(DocumentParseError::InvalidSpan)?;
        admit(0, end - start, max_bytes, "frontmatter bytes")?;
        if matches!(line_content(line), "---" | "...") {
            return Ok(DocumentEnvelope {
                bom,
                frontmatter: Some(start..end),
                yaml: Some(yaml_start..position),
                body: end..source.len(),
            });
        }
        position = end;
    }
    Err(DocumentParseError::UnclosedFrontmatter)
}

fn line_content(line: &str) -> &str {
    match line.strip_suffix('\n') {
        Some(line) => line.strip_suffix('\r').unwrap_or(line),
        None => line,
    }
}

enum FrameKind {
    Sequence,
    Mapping {
        pending_key: Option<NodeId>,
        keys: HashSet<String>,
    },
}

struct Frame {
    node: NodeId,
    kind: FrameKind,
}

fn parse_yaml<'a>(
    context: &SnapshotContext,
    source: &'a str,
    range: Range<usize>,
    limits: DocumentParseLimits,
    budget: &mut TextBudget<'_>,
    stats: &mut ParseStats,
) -> Result<Metadata<'a>> {
    let payload = &source[range.clone()];
    let options = yaml::options! {
        emit_comments: false,
        max_buffered_comment_events: 0,
        simple_key_max_lookahead: 1024,
        flow_nesting_limit: limits.max_yaml_depth.get(),
        block_nesting_limit: limits.max_yaml_depth.get(),
        max_directive_bytes: limits.max_frontmatter_bytes.get(),
        max_reserved_directive_params: limits.max_yaml_events.get(),
    };
    context.check()?;
    let mut parser = yaml::Parser::new_from_str_with_options(payload, options);
    let mut metadata = Metadata::default();
    let mut stack: Vec<Frame> = Vec::new();
    let mut documents = 0usize;
    let preceding_lines = source[..range.start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    loop {
        context.check()?;
        let Some(event) = parser.next() else { break };
        context.check()?;
        let (event, span) = match event {
            Ok(event) => event,
            Err(source) => {
                let marker = source.marker();
                let location = YamlLocation {
                    yaml_line: marker.line(),
                    yaml_column: marker.col(),
                    document_line: preceding_lines
                        .checked_add(marker.line())
                        .ok_or(DocumentParseError::InvalidSpan)?,
                };
                return Err(DocumentParseError::Yaml { source, location });
            }
        };
        stats.yaml_events = admit(stats.yaml_events, 1, limits.max_yaml_events, "yaml events")?;
        if event.anchor_id().is_some() {
            return Err(DocumentParseError::UnsupportedFeature("YAML anchor"));
        }
        if event.tag().is_some() {
            return Err(DocumentParseError::UnsupportedFeature("YAML tag"));
        }
        match event {
            yaml::Event::StreamStart | yaml::Event::StreamEnd | yaml::Event::DocumentEnd => {}
            yaml::Event::DocumentStart(..) => {
                documents = documents
                    .checked_add(1)
                    .ok_or(DocumentParseError::MultipleDocuments)?;
                if documents > 1 {
                    return Err(DocumentParseError::MultipleDocuments);
                }
            }
            yaml::Event::Alias(_) => {
                return Err(DocumentParseError::UnsupportedFeature("YAML alias"));
            }
            yaml::Event::Scalar(value, style, ..) => {
                admit(0, value.len(), limits.max_scalar_bytes, "scalar bytes")?;
                budget.charge(value.len())?;
                let span = source_span(
                    source,
                    span.byte_range().ok_or(DocumentParseError::InvalidSpan)?,
                    range.start,
                    payload.len(),
                )?;
                let node = MetadataNode {
                    span,
                    kind: MetadataKind::Scalar { value, style },
                };
                add_node(context, &mut metadata, &mut stack, node, budget)?;
            }
            yaml::Event::MappingStart(..) | yaml::Event::SequenceStart(..) => {
                let depth = admit(stack.len(), 1, limits.max_yaml_depth, "yaml depth")?;
                stats.yaml_depth = stats.yaml_depth.max(depth);
                let mapping = matches!(event, yaml::Event::MappingStart(..));
                let span = source_span(
                    source,
                    span.byte_range().ok_or(DocumentParseError::InvalidSpan)?,
                    range.start,
                    payload.len(),
                )?;
                let kind = if mapping {
                    MetadataKind::Mapping {
                        entries: Vec::new(),
                    }
                } else {
                    MetadataKind::Sequence { items: Vec::new() }
                };
                let id = add_node(
                    context,
                    &mut metadata,
                    &mut stack,
                    MetadataNode { span, kind },
                    budget,
                )?;
                reserve(&mut stack)?;
                stack.push(Frame {
                    node: id,
                    kind: if mapping {
                        FrameKind::Mapping {
                            pending_key: None,
                            keys: HashSet::new(),
                        }
                    } else {
                        FrameKind::Sequence
                    },
                });
            }
            yaml::Event::MappingEnd | yaml::Event::SequenceEnd => {
                let frame = stack.pop().ok_or(DocumentParseError::InvalidSpan)?;
                let matching = matches!(
                    (&frame.kind, &event),
                    (
                        FrameKind::Mapping {
                            pending_key: None,
                            ..
                        },
                        yaml::Event::MappingEnd
                    ) | (FrameKind::Sequence, yaml::Event::SequenceEnd)
                );
                if !matching {
                    return Err(DocumentParseError::InvalidSpan);
                }
                let closing = source_span(
                    source,
                    span.byte_range().ok_or(DocumentParseError::InvalidSpan)?,
                    range.start,
                    payload.len(),
                )?;
                let opening = metadata.nodes[frame.node.0].span.start;
                metadata.nodes[frame.node.0].span =
                    source_span(source, opening..closing.end, 0, source.len())?;
            }
            _ => return Err(DocumentParseError::UnsupportedFeature("YAML event")),
        }
    }
    if !stack.is_empty() {
        return Err(DocumentParseError::InvalidSpan);
    }
    context.check()?;
    Ok(metadata)
}

fn add_node<'a>(
    context: &SnapshotContext,
    metadata: &mut Metadata<'a>,
    stack: &mut [Frame],
    node: MetadataNode<'a>,
    budget: &mut TextBudget<'_>,
) -> Result<NodeId> {
    context.check()?;
    let id = NodeId(metadata.nodes.len());
    if let Some(frame) = stack.last_mut() {
        match &mut frame.kind {
            FrameKind::Sequence => {
                let MetadataKind::Sequence { items } = &mut metadata.nodes[frame.node.0].kind
                else {
                    return Err(DocumentParseError::InvalidSpan);
                };
                reserve(items)?;
                items.push(id);
            }
            FrameKind::Mapping { pending_key, keys } => {
                if let Some(key) = pending_key.take() {
                    let MetadataKind::Mapping { entries } = &mut metadata.nodes[frame.node.0].kind
                    else {
                        return Err(DocumentParseError::InvalidSpan);
                    };
                    reserve(entries)?;
                    entries.push((key, id));
                } else {
                    context.check()?;
                    let MetadataKind::Scalar { value, style } = &node.kind else {
                        return Err(DocumentParseError::InvalidFieldType("mapping key"));
                    };
                    if is_null(value, *style) {
                        return Err(DocumentParseError::InvalidFieldType("mapping key"));
                    }
                    if value == "<<" {
                        return Err(DocumentParseError::UnsupportedFeature("YAML merge key"));
                    }
                    if keys.contains(value.as_ref()) {
                        return Err(DocumentParseError::DuplicateKey);
                    }
                    let copy = budget.copy(value)?;
                    keys.try_reserve(1).map_err(|_| {
                        DocumentParseError::Request(io::ErrorKind::OutOfMemory.into())
                    })?;
                    keys.insert(copy);
                    *pending_key = Some(id);
                }
            }
        }
    } else {
        if metadata.root.is_some() {
            return Err(DocumentParseError::MultipleDocuments);
        }
        if !matches!(node.kind, MetadataKind::Mapping { .. }) {
            return Err(DocumentParseError::InvalidFieldType("frontmatter"));
        }
        metadata.root = Some(id);
    }
    reserve(&mut metadata.nodes)?;
    metadata.nodes.push(node);
    Ok(id)
}

fn is_null(value: &str, style: ScalarStyle) -> bool {
    style == ScalarStyle::Plain && matches!(value, "" | "~" | "null" | "Null" | "NULL")
}

fn editorial_field<'a>(
    context: &SnapshotContext,
    metadata: &'a Metadata<'_>,
    name: &'static str,
) -> Result<Option<&'a str>> {
    let Some(root) = metadata.root else {
        return Ok(None);
    };
    let MetadataKind::Mapping { entries } = &metadata.nodes[root.0].kind else {
        return Err(DocumentParseError::InvalidFieldType("frontmatter"));
    };
    for (key, value) in entries {
        context.check()?;
        let MetadataKind::Scalar { value: key, .. } = &metadata.nodes[key.0].kind else {
            return Err(DocumentParseError::InvalidFieldType("mapping key"));
        };
        if key != name {
            continue;
        }
        let MetadataKind::Scalar { value, style } = &metadata.nodes[value.0].kind else {
            return Err(DocumentParseError::InvalidFieldType(name));
        };
        return Ok((!is_null(value, *style) && !value.trim().is_empty()).then_some(value.trim()));
    }
    Ok(None)
}

#[derive(Default)]
struct Editorial {
    title: Option<String>,
    summary: Option<String>,
}

fn parse_markdown<'a>(
    context: &SnapshotContext,
    source: &'a str,
    body: Range<usize>,
    limits: DocumentParseLimits,
    budget: &mut TextBudget<'_>,
    stats: &mut ParseStats,
) -> Result<(Vec<MarkdownEvent<'a>>, Editorial)> {
    context.check()?;
    let mut parser = Parser::new_ext(
        &source[body.clone()],
        Options::ENABLE_TABLES | Options::ENABLE_WIKILINKS,
    )
    .into_offset_iter();
    context.check()?;
    let mut events = Vec::new();
    let mut editorial = Editorial::default();
    let mut title_buffer = None;
    let mut summary_buffer = None;
    let mut excluded = 0usize;
    loop {
        context.check()?;
        let Some((event, local_span)) = parser.next() else {
            break;
        };
        context.check()?;
        stats.markdown_events = admit(
            stats.markdown_events,
            1,
            limits.max_markdown_events,
            "markdown events",
        )?;
        let span = source_span(source, local_span, body.start, body.len())?;
        charge_markdown(&event, budget)?;
        match &event {
            Event::Start(tag) => {
                if matches!(tag, Tag::Link { .. }) {
                    stats.links = admit(stats.links, 1, limits.max_links, "links")?;
                }
                if matches!(
                    tag,
                    Tag::Heading {
                        level: HeadingLevel::H1,
                        ..
                    }
                ) && editorial.title.is_none()
                {
                    title_buffer = Some(String::new());
                }
                if matches!(tag, Tag::Paragraph) && excluded == 0 && editorial.summary.is_none() {
                    summary_buffer = Some(String::new());
                }
                if matches!(
                    tag,
                    Tag::Heading { .. } | Tag::CodeBlock(_) | Tag::Table(_) | Tag::HtmlBlock
                ) {
                    excluded = excluded
                        .checked_add(1)
                        .ok_or(DocumentParseError::LimitExceeded("markdown events"))?;
                }
            }
            Event::End(tag) => {
                if matches!(tag, TagEnd::Heading(HeadingLevel::H1))
                    && let Some(buffer) = title_buffer
                        .take()
                        .filter(|text: &String| !text.trim().is_empty())
                {
                    editorial.title = Some(buffer);
                }
                if matches!(tag, TagEnd::Paragraph)
                    && let Some(buffer) = summary_buffer.take()
                {
                    editorial.summary = Some(buffer);
                }
                if matches!(
                    tag,
                    TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::Table | TagEnd::HtmlBlock
                ) {
                    excluded = excluded
                        .checked_sub(1)
                        .ok_or(DocumentParseError::InvalidSpan)?;
                }
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some(buffer) = &mut title_buffer {
                    budget.append(buffer, text)?;
                }
                if let Some(buffer) = &mut summary_buffer {
                    budget.append(buffer, text)?;
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(buffer) = &mut title_buffer {
                    budget.append(buffer, " ")?;
                }
                if let Some(buffer) = &mut summary_buffer {
                    budget.append(buffer, " ")?;
                }
            }
            _ => {}
        }
        reserve(&mut events)?;
        events.push(MarkdownEvent { event, span });
    }
    context.check()?;
    Ok((events, editorial))
}

fn charge_markdown(event: &Event<'_>, budget: &mut TextBudget<'_>) -> Result<()> {
    match event {
        Event::Text(text)
        | Event::Code(text)
        | Event::InlineMath(text)
        | Event::DisplayMath(text)
        | Event::Html(text)
        | Event::InlineHtml(text)
        | Event::FootnoteReference(text) => budget.charge(text.len())?,
        Event::Start(tag) => match tag {
            Tag::Link {
                dest_url,
                title,
                id,
                ..
            }
            | Tag::Image {
                dest_url,
                title,
                id,
                ..
            } => {
                budget.charge(dest_url.len())?;
                budget.charge(title.len())?;
                budget.charge(id.len())?;
            }
            Tag::CodeBlock(CodeBlockKind::Fenced(info)) | Tag::FootnoteDefinition(info) => {
                budget.charge(info.len())?
            }
            Tag::Heading {
                id, classes, attrs, ..
            } => {
                if let Some(id) = id {
                    budget.charge(id.len())?;
                }
                for class in classes {
                    budget.charge(class.len())?;
                }
                for (key, value) in attrs {
                    budget.charge(key.len())?;
                    if let Some(value) = value {
                        budget.charge(value.len())?;
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
    Ok(())
}

fn prefix_boundary(text: &str, maximum: usize) -> usize {
    let mut end = text.len().min(maximum);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::{Duration, Instant};

    fn cap(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).unwrap()
    }
    fn limits() -> DocumentParseLimits {
        DocumentParseLimits {
            max_document_bytes: cap(100_000),
            max_input_bytes: cap(100_100),
            max_frontmatter_bytes: cap(80_000),
            max_yaml_events: cap(20_000),
            max_markdown_events: cap(20_000),
            max_yaml_depth: cap(256),
            max_scalar_bytes: cap(10_000),
            max_links: cap(1_000),
            max_projection_bytes: cap(1_000_000),
            max_summary_bytes: cap(1_000),
        }
    }
    fn context() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        )
    }
    fn parse(source: &str) -> ParsedDocument<'_> {
        parse_document(&context(), source.as_bytes(), "fallback", limits()).unwrap()
    }
    fn field<'a>(document: &'a ParsedDocument<'_>, name: &str) -> &'a MetadataNode<'a> {
        let metadata = &document.metadata;
        let MetadataKind::Mapping { entries } =
            &metadata.node(metadata.root.unwrap()).unwrap().kind
        else {
            panic!("mapping root")
        };
        let (_, value) = entries.iter().find(|(key, _)| matches!(&metadata.node(*key).unwrap().kind, MetadataKind::Scalar { value, .. } if value == name)).unwrap();
        metadata.node(*value).unwrap()
    }
    fn limit_error(result: Result<ParsedDocument<'_>>, name: &str) {
        assert!(matches!(result, Err(DocumentParseError::LimitExceeded(actual)) if actual == name));
    }

    #[test]
    fn envelope_preserves_bom_crlf_delimiters_and_byte_offsets() {
        let source = include!("../tests/fixtures/document/unicode-crlf.rs");
        let doc = parse(source);
        assert!(std::ptr::eq(doc.source.as_ptr(), source.as_ptr()));
        assert_eq!(doc.source.as_bytes(), source.as_bytes());
        assert_eq!(doc.envelope.bom, Some(0..3));
        assert_eq!(
            &source[doc.envelope.frontmatter.unwrap()],
            "---\r\ntitle: Café\r\n...\r\n"
        );
        assert_eq!(&source[doc.envelope.yaml.unwrap()], "title: Café\r\n");
        assert_eq!(&source[doc.envelope.body], "# Olá\r\n");
        assert_eq!(doc.title, "Café");
        let text = doc
            .markdown
            .iter()
            .find(|event| matches!(&event.event, Event::Text(text) if text.as_ref() == "Olá"))
            .unwrap();
        assert_eq!(text.span, 29..33);
        assert_eq!(&source[text.span.clone()], "Olá");
        let heading = doc
            .markdown
            .iter()
            .find(|event| matches!(event.event, Event::Start(Tag::Heading { .. })))
            .unwrap();
        assert_eq!(heading.span, 27..35);
        for event in &doc.markdown {
            assert!(source.get(event.span.clone()).is_some());
        }
    }

    #[test]
    fn envelope_exact_lines_empty_roots_and_eof_are_explicit() {
        for source in [
            "",
            "prose",
            " ---\n",
            "--- \n",
            "---prosa\n",
            "\u{feff}\u{feff}---\n",
        ] {
            assert!(parse(source).envelope.frontmatter.is_none(), "{source:?}");
        }
        for source in ["---", "---\n", "---\r\n# only comment\r\n"] {
            assert!(matches!(
                parse_document(&context(), source.as_bytes(), "f", limits()),
                Err(DocumentParseError::UnclosedFrontmatter)
            ));
        }
        for source in ["---\n---", "---\r\n...", "---\n# comment\n \n---\n"] {
            let doc = parse(source);
            assert!(doc.metadata.root.is_none());
            assert!(doc.envelope.frontmatter.is_some());
            assert!(doc.envelope.body.is_empty());
        }
        assert!(parse("---\n{}\n---\n").metadata.root.is_some());
    }

    #[test]
    fn metadata_keeps_decoded_text_styles_synthetic_scalars_and_collection_spans() {
        let source = include_str!("../tests/fixtures/document/metadata.md");
        let doc = parse(source);
        assert_eq!(doc.title, "Café: graphit");
        assert_eq!(doc.summary, "first line second line");
        assert!(
            matches!(&field(&doc, "numeric").kind, MetadataKind::Scalar { value, style: ScalarStyle::Plain } if value == "0xFf001122")
        );
        assert!(
            matches!(&field(&doc, "quoted_null").kind, MetadataKind::Scalar { value, style: ScalarStyle::SingleQuoted } if value == "null")
        );
        let root = doc.metadata.node(doc.metadata.root.unwrap()).unwrap();
        assert_eq!(root.span.start, doc.envelope.yaml.as_ref().unwrap().start);
        for node in doc.metadata.nodes() {
            let raw = &source[node.span.clone()];
            match &node.kind {
                MetadataKind::Mapping { .. } if raw.starts_with('{') => assert!(raw.ends_with('}')),
                MetadataKind::Sequence { .. } if raw.starts_with('[') => {
                    assert!(raw.ends_with(']'))
                }
                _ => {}
            }
        }
        let synthetic = parse("---\na:\nb: {c:}\nd:\n  -\ne: ~\nf: '~'\n---\n");
        let nodes = synthetic.metadata.nodes();
        let ranges: Vec<_> = nodes
            .iter()
            .filter_map(|node| match &node.kind {
                MetadataKind::Scalar {
                    value,
                    style: ScalarStyle::Plain,
                } if value == "~" => Some(&synthetic.source[node.span.clone()]),
                _ => None,
            })
            .collect();
        assert!(ranges.iter().any(|text| text.is_empty() || *text == ":"));
        let MetadataKind::Sequence { items } = &field(&synthetic, "d").kind else {
            panic!("block sequence")
        };
        assert_eq!(items.len(), 1);
        let item = synthetic.metadata.node(items[0]).unwrap();
        assert!(matches!(
            &item.kind,
            MetadataKind::Scalar { value, style: ScalarStyle::Plain } if value == "~"
        ));
        assert_eq!(item.span, 21..21);
        assert_eq!(synthetic.source.as_bytes()[item.span.start - 1], b'-');
        assert_eq!(synthetic.source.as_bytes()[item.span.start], b'\n');
        assert_eq!(&synthetic.source[item.span.clone()], "");
        assert!(ranges.contains(&"~"));
        assert_eq!(field(&synthetic, "e").span, 25..26);
        assert!(matches!(
            &field(&synthetic, "e").kind,
            MetadataKind::Scalar { value, style: ScalarStyle::Plain } if value == "~"
        ));
        assert_eq!(field(&synthetic, "f").span, 30..33);
        assert!(
            matches!(&field(&synthetic, "f").kind, MetadataKind::Scalar { value, style: ScalarStyle::SingleQuoted } if value == "~")
        );
    }

    #[test]
    fn references_remain_distinct_authored_syntax_without_records_coercion() {
        let absent = parse("---\ntitle: x\n---\n");
        assert_eq!(absent.metadata.nodes().len(), 3);
        for (value, kind) in [
            ("[]", "sequence"),
            ("[a, b]", "sequence"),
            ("null", "scalar"),
            ("{x: y}", "mapping"),
        ] {
            let source = format!("---\nreferences: {value}\n---\n");
            let doc = parse(&source);
            let actual = match field(&doc, "references").kind {
                MetadataKind::Scalar { .. } => "scalar",
                MetadataKind::Sequence { .. } => "sequence",
                MetadataKind::Mapping { .. } => "mapping",
            };
            assert_eq!(actual, kind);
        }
    }

    #[test]
    fn yaml_rejects_duplicates_complex_keys_roots_and_explicit_features() {
        for yaml in ["a: 1\na: 2\n", "nested: {a: 1, \"\\u0061\": 2}\n"] {
            let source = format!("---\n{yaml}---\n");
            assert!(matches!(
                parse_document(&context(), source.as_bytes(), "f", limits()),
                Err(DocumentParseError::DuplicateKey)
            ));
        }
        for yaml in [
            "[a]\n",
            "scalar\n",
            "? [a, b]\n: x\n",
            "null: x\n",
            "title: []\n",
            "description: {}\n",
        ] {
            let source = format!("---\n{yaml}---\n");
            assert!(
                matches!(
                    parse_document(&context(), source.as_bytes(), "f", limits()),
                    Err(DocumentParseError::InvalidFieldType(_))
                ),
                "{yaml}"
            );
        }
        for yaml in [
            "a: &unused x\n",
            "a: !!str x\n",
            "a: !!int 2\n",
            "a: !custom x\n",
            "<<: x\n",
            "'<<': x\n",
        ] {
            let source = format!("---\n{yaml}---\n");
            assert!(
                matches!(
                    parse_document(&context(), source.as_bytes(), "f", limits()),
                    Err(DocumentParseError::UnsupportedFeature(_))
                ),
                "{yaml}"
            );
        }
        assert!(matches!(
            parse_document(&context(), b"---\na: *missing\n---\n", "f", limits()),
            Err(DocumentParseError::Yaml { .. })
        ));
        assert_eq!(
            parse("---\na: '& * <<'\nA: 1\n'null': x\n'': y\n---\n")
                .metadata
                .nodes()
                .len(),
            9
        );
    }

    #[test]
    fn yaml_multiple_documents_and_malformed_inputs_fail_as_whole_documents() {
        assert!(matches!(
            parse_document(
                &context(),
                b"---\na: 1\n--- # second\nb: 2\n---\n",
                "f",
                limits()
            ),
            Err(DocumentParseError::MultipleDocuments)
        ));
        for source in ["---\na: [secret\n---\n", "---\nreferences: [secret\n---\n"] {
            let err = parse_document(&context(), source.as_bytes(), "f", limits()).unwrap_err();
            assert!(matches!(err, DocumentParseError::Yaml { .. }));
            assert!(err.source().is_some());
            assert!(!err.to_string().contains("secret"));
        }
    }

    #[test]
    fn yaml_unicode_crlf_errors_preserve_native_location_units() {
        let source = "\u{feff}---\r\né: [é, }\r\n---\r\n";
        let DocumentParseError::Yaml {
            source: error,
            location,
        } = parse_document(&context(), source.as_bytes(), "f", limits()).unwrap_err()
        else {
            panic!("native YAML error")
        };
        assert_eq!(location.yaml_line, error.marker().line());
        assert_eq!(location.yaml_column, error.marker().col());
        assert_eq!(location.document_line, location.yaml_line + 1);
        assert_eq!(location.yaml_line, 1);
        assert_eq!(location.yaml_column, 3);
        assert!(matches!(
            error.kind(),
            yaml::ErrorKind::MismatchedFlowCollectionEnd {
                open: '[',
                close: '}'
            }
        ));
        let yaml = &source[8..];
        let opening = yaml.find('[').unwrap();
        assert_eq!(opening, 4);
        assert_eq!(yaml[..opening].chars().count(), location.yaml_column);
        assert_eq!(source.as_bytes()[8 + opening], b'[');
        assert_eq!(location.document_line, 2);
    }

    #[test]
    fn editorial_precedence_null_policy_and_first_eligible_paragraph_are_explicit() {
        for value in ["null", "~", "''", "'  '"] {
            let source = format!(
                "---\ntitle: {value}\ndescription: {value}\n---\n# \n\n# *Useful* `title` &amp; é\n\nfirst `text`\nnext  \nlast\n\nsecond\n"
            );
            let doc = parse(&source);
            assert_eq!(doc.title, "Useful title & é");
            assert_eq!(doc.summary, "first text next last");
        }
        assert_eq!(
            parse("---\ntitle: 'null'\ndescription: 'null'\n---\n# body\n\nbody\n").title,
            "null"
        );
        assert_eq!(parse("<span></span>\n\nlater\n").summary, "");
        assert_eq!(parse("prose\n").title, "fallback");
        assert_eq!(
            parse_document(&context(), b"", "  fallback  ", limits())
                .unwrap()
                .title,
            "fallback"
        );
    }

    #[test]
    fn markdown_events_keep_protected_syntax_and_count_links_without_resolution() {
        let source = include_str!("../tests/fixtures/document/rich.md");
        let doc = parse(source);
        assert_eq!(doc.title, "A B & C D");
        assert!(doc.summary.starts_with("prose code"));
        assert!(
            doc.markdown
                .iter()
                .any(|event| matches!(event.event, Event::Start(Tag::Table(_))))
        );
        assert!(
            doc.markdown
                .iter()
                .any(|event| matches!(event.event, Event::Start(Tag::CodeBlock(_))))
        );
        assert!(
            doc.markdown
                .iter()
                .any(|event| matches!(event.event, Event::Html(_)))
        );
        let links = doc
            .markdown
            .iter()
            .filter(|event| matches!(event.event, Event::Start(Tag::Link { .. })))
            .count();
        assert_eq!(doc.stats.links, links);
        assert!(links >= 7);
        assert_eq!(doc.source.as_bytes(), source.as_bytes());
        for event in &doc.markdown {
            assert!(source.get(event.span.clone()).is_some());
        }
        assert!(
            doc.markdown
                .iter()
                .any(|event| matches!(&event.event, Event::Text(text) if text.contains("unknown")))
        );
    }

    #[test]
    fn every_admission_cap_accepts_exact_and_rejects_one_below_required_work() {
        let input = "---\na: {b: é}\n---\n[x](u) [x](u)\n";
        let ctx = context();
        let baseline = parse_document(&ctx, input.as_bytes(), "F", limits()).unwrap();
        assert_eq!(
            baseline.stats,
            ParseStats {
                yaml_events: 11,
                markdown_events: 9,
                yaml_depth: 2,
                links: 2,
                projection_bytes: 18
            }
        );
        type CapCase = (&'static str, usize, fn(&mut DocumentParseLimits, usize));
        let cases: [CapCase; 9] = [
            ("document bytes", input.len(), |l, n| {
                l.max_document_bytes = cap(n)
            }),
            ("input bytes", input.len() + 1, |l, n| {
                l.max_input_bytes = cap(n)
            }),
            (
                "frontmatter bytes",
                baseline.envelope.frontmatter.as_ref().unwrap().len(),
                |l, n| l.max_frontmatter_bytes = cap(n),
            ),
            ("yaml events", baseline.stats.yaml_events, |l, n| {
                l.max_yaml_events = cap(n)
            }),
            ("markdown events", baseline.stats.markdown_events, |l, n| {
                l.max_markdown_events = cap(n)
            }),
            ("yaml depth", baseline.stats.yaml_depth, |l, n| {
                l.max_yaml_depth = cap(n)
            }),
            ("scalar bytes", 2, |l, n| l.max_scalar_bytes = cap(n)),
            ("links", 2, |l, n| l.max_links = cap(n)),
            (
                "projection bytes",
                baseline.stats.projection_bytes,
                |l, n| l.max_projection_bytes = cap(n),
            ),
        ];
        for (name, required, set) in cases {
            assert!(required > 1, "{name}");
            let mut exact = limits();
            set(&mut exact, required);
            assert!(
                parse_document(&ctx, input.as_bytes(), "F", exact).is_ok(),
                "exact {name}"
            );
            set(&mut exact, required - 1);
            let result = parse_document(&ctx, input.as_bytes(), "F", exact);
            if name == "yaml depth" {
                match result.unwrap_err() {
                    DocumentParseError::LimitExceeded("yaml depth") => {}
                    DocumentParseError::Yaml { source, .. } => assert!(matches!(
                        source.kind(),
                        yaml::ErrorKind::RecursionLimitExceeded
                    )),
                    other => panic!("unexpected depth error: {other}"),
                }
            } else {
                limit_error(result, name);
            }
        }
    }

    #[test]
    fn projection_counts_borrowed_text_key_copies_and_editorial_copies() {
        let doc = parse_document(&context(), b"---\na: x\n---\nbody\n", "F", limits()).unwrap();
        // YAML a+x, duplicate-key scratch a, Markdown body, prose buffer body,
        // title F and summary body: 2+1+4+4+1+4.
        assert_eq!(doc.stats.projection_bytes, 16);
        let mut bound = limits();
        bound.max_projection_bytes = cap(15);
        limit_error(
            parse_document(&context(), b"---\na: x\n---\nbody\n", "F", bound),
            "projection bytes",
        );
    }

    #[test]
    fn summary_truncation_is_an_explicit_utf8_prefix_for_description_and_prose() {
        for source in ["ééé\n", "---\ndescription: ééé\n---\nbody\n"] {
            for (maximum, summary, truncated) in [
                (1, "", true),
                (3, "é", true),
                (6, "ééé", false),
                (7, "ééé", false),
            ] {
                let mut bound = limits();
                bound.max_summary_bytes = cap(maximum);
                let doc = parse_document(&context(), source.as_bytes(), "F", bound).unwrap();
                assert_eq!(doc.summary, summary);
                assert_eq!(doc.summary_truncated, truncated);
            }
        }
    }

    #[test]
    fn mixed_container_depth_and_flat_arena_are_bounded_without_recursive_drop() {
        let mut bound = limits();
        bound.max_yaml_depth = cap(2);
        limit_error(
            parse_document(&context(), b"---\na:\n  b: [x]\n---\n", "f", bound),
            "yaml depth",
        );
        let source = format!("---\na: {}x{}\n---\n", "[".repeat(160), "]".repeat(160));
        let doc = parse(&source);
        assert_eq!(doc.stats.yaml_depth, 161);
        assert_eq!(doc.metadata.nodes().len(), 163);
        drop(doc);
    }

    #[test]
    fn cancellation_deadline_and_invalid_utf8_preserve_errors_without_io() {
        let ctx = context();
        ctx.cancellation().cancel();
        let DocumentParseError::Request(err) =
            parse_document(&ctx, b"\xff", "f", limits()).unwrap_err()
        else {
            panic!("cancelled")
        };
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        let expired = SnapshotContext::new(
            Instant::now() - Duration::from_millis(1),
            Cancellation::default(),
        );
        let DocumentParseError::Request(err) =
            parse_document(&expired, b"", "f", limits()).unwrap_err()
        else {
            panic!("expired")
        };
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        let original = context();
        parse_document(&original, b"body\n", "f", limits()).unwrap();
        let DocumentParseError::InvalidUtf8(err) =
            parse_document(&original, b"a\xff", "f", limits()).unwrap_err()
        else {
            panic!("invalid UTF8")
        };
        assert_eq!(err.valid_up_to(), 1);
    }

    #[test]
    fn overflow_and_invalid_ranges_fail_before_allocation_or_slicing() {
        assert!(matches!(
            admit(usize::MAX, 1, cap(usize::MAX), "input bytes"),
            Err(DocumentParseError::LimitExceeded("input bytes"))
        ));
        assert!(matches!(
            source_span("é", 1..2, 0, 2),
            Err(DocumentParseError::InvalidSpan)
        ));
        assert!(matches!(
            source_span("a", 1..1, usize::MAX, 1),
            Err(DocumentParseError::InvalidSpan)
        ));
        assert!(matches!(
            source_span("a", 0..2, 0, 1),
            Err(DocumentParseError::InvalidSpan)
        ));
        assert_eq!(source_span("a", 1..1, 0, 1).unwrap(), 1..1);
    }

    #[test]
    fn native_scanner_depth_error_preserves_origin_and_text_overflow_does_not_mutate_ledger() {
        let mut bound = limits();
        bound.max_yaml_depth = cap(1);
        // A flow mapping at the root keeps a possible simple key pending,
        // making the scanner admit the nested opener during lookahead.
        let err = parse_document(&context(), b"---\n{a: [x]}\n---\n", "f", bound).unwrap_err();
        let DocumentParseError::Yaml { source, location } = &err else {
            panic!("expected native lookahead depth guard: {err}")
        };
        assert!(matches!(
            source.kind(),
            yaml::ErrorKind::RecursionLimitExceeded
        ));
        assert!(err.source().is_some());
        assert_eq!(location.yaml_line, source.marker().line());
        assert_eq!(location.yaml_column, source.marker().col());
        let ctx = context();
        let mut budget = TextBudget {
            context: &ctx,
            limit: cap(usize::MAX),
            used: usize::MAX,
        };
        assert!(matches!(
            budget.charge(1),
            Err(DocumentParseError::LimitExceeded("projection bytes"))
        ));
        assert_eq!(budget.used, usize::MAX);
    }
}
