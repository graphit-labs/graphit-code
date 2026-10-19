//! File-backed Markdown export. The caller owns the authorized Source cursor and FS authority.
use crate::{
    document::{DocumentParseError, DocumentParseLimits, parse_document},
    document_communities::{
        DocumentCommunities, DocumentCommunityError, classify_document_communities,
    },
    document_source::{DocumentSourceError, native_metadata, write_native_metadata},
    native_document_mapping::{NativeAuthoredMappingError, local_target},
    okf_export::KnowledgeTextExportFormat,
    references::{
        Direction, PageReferences, ReferenceError, ReferenceGraph, ReferenceHit, ReferenceLimits,
    },
};
use graphit_core::{
    artifact_store::{PayloadMode, SourceSnapshot},
    file_io::SnapshotContext,
};
use graphit_data::knowledge_index::KnowledgeDocument;
use graphit_fs::NodeKind;
use pulldown_cmark::{CowStr, Event, HeadingLevel, LinkType, Options, Tag, TagEnd};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, TryReserveError},
    error::Error,
    fmt,
    fs::{self, File},
    io::{self, Read, Write},
    num::NonZeroUsize,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

#[derive(Clone, Copy)]
pub struct MarkdownExportModes {
    pub directory: PayloadMode,
    pub file: PayloadMode,
}
#[derive(Clone, Copy, Debug)]
pub struct MarkdownExportLimits {
    pub max_file_bytes: NonZeroUsize,
    pub document: DocumentParseLimits,
    pub references: ReferenceLimits,
}
/// Keep this owner alive through validation and publication of the prepared tree.
pub struct MarkdownExportPlan {
    pub payload: SourceSnapshot,
    pub communities: DocumentCommunities,
    staging: TempDir,
}
impl MarkdownExportPlan {
    /// Files remain owned by this plan; this path conveys no filesystem authority.
    pub fn directory(&self) -> PathBuf {
        self.staging.path().join("payload")
    }
    /// Release the private spool after publication; report the actual cleanup result.
    pub fn close(self) -> (DocumentCommunities, io::Result<()>) {
        let Self {
            payload,
            communities,
            staging,
        } = self;
        drop(payload);
        (communities, staging.close())
    }
}

struct Page {
    title: String,
    targets: Vec<String>,
    prefix_bytes: usize,
    fallback_title: String,
}
/// One document at a time is rendered to a private file. Only navigation metadata
/// remains in memory. `finish` is called by the consumer after its validated EOF.
pub struct MarkdownExportStage<'original> {
    original: &'original SnapshotContext,
    staging: TempDir,
    pages: BTreeMap<String, Page>,
    modes: MarkdownExportModes,
    limits: MarkdownExportLimits,
    format: KnowledgeTextExportFormat,
}
impl<'original> MarkdownExportStage<'original> {
    pub fn new_in(
        original: &'original SnapshotContext,
        parent: &Path,
        modes: MarkdownExportModes,
        limits: MarkdownExportLimits,
        format: KnowledgeTextExportFormat,
    ) -> Result<Self> {
        check(original)?;
        let staging = tempfile::tempdir_in(parent).map_err(MarkdownExportError::Payload)?;
        graphit_fs::create_private_directory(&staging.path().join("records"))
            .map_err(MarkdownExportError::Payload)?;
        graphit_fs::create_private_directory(&staging.path().join("payload"))
            .map_err(MarkdownExportError::Payload)?;
        graphit_fs::create_private_directory(&staging.path().join("payload/pages"))
            .map_err(MarkdownExportError::Payload)?;
        Ok(Self {
            original,
            staging,
            pages: BTreeMap::new(),
            modes,
            limits,
            format,
        })
    }
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }
    fn recheck(&self, original: &SnapshotContext) -> Result<()> {
        check(self.original)?;
        if !self.original.same_request(original) {
            return Err(MarkdownExportError::References(
                ReferenceError::DifferentRequest,
            ));
        }
        check(original)
    }
    pub fn push(&mut self, original: &SnapshotContext, d: KnowledgeDocument) -> Result<()> {
        self.recheck(original)?;
        self.format.validate_document(&d)?;
        if self.pages.contains_key(&d.slug) {
            return Err(MarkdownExportError::References(
                ReferenceError::DuplicatePage,
            ));
        }
        // Validate the page and its producer-resolved identifiers before staging.
        let targets = d
            .resolved_wiki_targets
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        ReferenceGraph::build(
            original,
            &[PageReferences {
                slug: &d.slug,
                title: &d.title,
                targets: &targets,
            }],
            self.limits.references,
        )
        .map_err(MarkdownExportError::References)?;
        let obsidian = matches!(self.format, KnowledgeTextExportFormat::Obsidian { .. });
        // Obsidian needs the complete exported roster. Its only parse happens
        // at finish, one file at a time; admit the raw spool before any write.
        if obsidian && d.body.len() > self.limits.document.max_document_bytes.get() {
            return Err(MarkdownExportError::Parse(
                DocumentParseError::LimitExceeded("document bytes"),
            ));
        }
        let parsed = if obsidian {
            None
        } else {
            Some(
                parse_document(
                    original,
                    d.body.as_bytes(),
                    &d.recipe_parameters.fallback_title,
                    self.limits.document,
                )
                .map_err(MarkdownExportError::Parse)?,
            )
        };
        let file = graphit_fs::create_private_file(
            &self.staging.path().join("records").join(filename(&d.slug)),
        )
        .map_err(MarkdownExportError::Payload)?;
        let mut writer = ExportWriter::new(original, self.limits, io::BufWriter::new(file));
        write_header(&mut writer, &d, &self.format)?;
        heading(&mut writer, HeadingLevel::H1, title(&d))?;
        writer.append(b"\n\n")?;
        let prefix_bytes = writer.written;
        if let Some(parsed) = parsed {
            writer.append(parsed.source[parsed.envelope.body].as_bytes())?;
            writer.finish()?;
        } else {
            // Raw frontmatter is private input, not final output bytes. The
            // final ExportWriter accounts for the rendered body at finish.
            let mut raw = writer.finish()?;
            for bytes in d.body.as_bytes().chunks(8192) {
                check(original)?;
                raw.write_all(bytes).map_err(MarkdownExportError::Payload)?;
            }
            check(original)?;
            raw.flush().map_err(MarkdownExportError::Payload)?;
        }
        self.recheck(original)?;
        self.pages.insert(
            d.slug,
            Page {
                title: d.title,
                targets: d.resolved_wiki_targets,
                prefix_bytes,
                fallback_title: d.recipe_parameters.fallback_title,
            },
        );
        Ok(())
    }
    pub fn finish(self, original: &SnapshotContext) -> Result<MarkdownExportPlan> {
        self.recheck(original)?;
        if self.pages.is_empty() {
            return Err(MarkdownExportError::NoDocuments);
        }
        let targets = self
            .pages
            .values()
            .map(|p| p.targets.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let pages = self
            .pages
            .iter()
            .zip(&targets)
            .map(|((slug, p), targets)| PageReferences {
                slug,
                title: &p.title,
                targets,
            })
            .collect::<Vec<_>>();
        let graph = ReferenceGraph::build(original, &pages, self.limits.references)
            .map_err(MarkdownExportError::References)?;
        let communities = classify_document_communities(original, &graph)
            .map_err(MarkdownExportError::Communities)?;
        let output = self.staging.path().join("payload");
        let index = output.join("index.md");
        let mut writer = ExportWriter::new(
            original,
            self.limits,
            io::BufWriter::new(
                graphit_fs::create_private_file(&index).map_err(MarkdownExportError::Payload)?,
            ),
        );
        if self.format.is_okf() {
            write_frontmatter(&mut writer, &serde_json::json!({"okf_version": "0.2"}))?;
        }
        write_index(
            &mut writer,
            self.pages
                .iter()
                .map(|(slug, page)| (slug.as_str(), page.title.as_str())),
            self.pages.len(),
        )?;
        writer.finish()?;
        set_mode(&index, self.modes.file)?;
        for (document, (slug, page)) in self.pages.iter().enumerate() {
            self.recheck(original)?;
            let path = output.join("pages").join(filename(slug));
            let mut reader = File::open(self.staging.path().join("records").join(filename(slug)))
                .map_err(MarkdownExportError::Payload)?;
            let mut writer = ExportWriter::new(
                original,
                self.limits,
                io::BufWriter::new(
                    graphit_fs::create_private_file(&path).map_err(MarkdownExportError::Payload)?,
                ),
            );
            copy_checked(
                &mut Read::by_ref(&mut reader).take(page.prefix_bytes as u64),
                &mut writer,
            )?;
            navigation(
                &mut writer,
                "References",
                graph
                    .direct_neighbors(original, slug, Direction::Outbound)
                    .map_err(MarkdownExportError::References)?,
            )?;
            navigation(
                &mut writer,
                "Backlinks",
                graph
                    .direct_neighbors(original, slug, Direction::Inbound)
                    .map_err(MarkdownExportError::References)?,
            )?;
            heading(&mut writer, HeadingLevel::H2, "Content")?;
            writer.append(b"\n\n")?;
            if matches!(self.format, KnowledgeTextExportFormat::Obsidian { .. }) {
                obsidian_body(&mut reader, &mut writer, page, &self.pages, slug, document)?;
            } else {
                // Markdown and OKF preserve the literal authored suffix.
                copy_checked(&mut reader, &mut writer)?;
            }
            writer.finish()?;
            set_mode(&path, self.modes.file)?;
        }
        set_mode(&output.join("pages"), self.modes.directory)?;
        set_mode(&output, self.modes.directory)?;
        self.recheck(original)?;
        let payload = SourceSnapshot::capture_generated(&output, NodeKind::Directory, original)
            .map_err(MarkdownExportError::Payload)?;
        check(original)?;
        Ok(MarkdownExportPlan {
            payload,
            communities,
            staging: self.staging,
        })
    }
}
fn set_mode(path: &Path, mode: PayloadMode) -> Result<()> {
    let mut permissions = fs::metadata(path)
        .map_err(MarkdownExportError::Payload)?
        .permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(
            mode.unix_bits()
                .ok_or_else(|| MarkdownExportError::Payload(io::ErrorKind::InvalidInput.into()))?,
        );
    }
    #[cfg(windows)]
    {
        permissions.set_readonly(
            mode.windows_readonly()
                .ok_or_else(|| MarkdownExportError::Payload(io::ErrorKind::InvalidInput.into()))?,
        );
    }
    fs::set_permissions(path, permissions).map_err(MarkdownExportError::Payload)
}
#[derive(Debug)]
pub enum MarkdownExportError {
    Request(io::Error),
    Source(DocumentSourceError),
    Parse(DocumentParseError),
    Target(Box<NativeAuthoredMappingError>),
    References(ReferenceError),
    Communities(DocumentCommunityError),
    Serialization(pulldown_cmark_to_cmark::Error),
    Metadata(serde_json::Error),
    InvalidOkfMetadata(&'static str),
    Payload(io::Error),
    NoDocuments,
    InvalidDisplayText,
    Limit(&'static str),
    Overflow(&'static str),
    Allocation(TryReserveError),
}
impl fmt::Display for MarkdownExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(_) => f.write_str("Knowledge Markdown request failed"),
            Self::Source(_) => f.write_str("Knowledge Markdown metadata failed"),
            Self::Parse(_) => f.write_str("Knowledge Markdown body parsing failed"),
            Self::Target(_) => f.write_str("Knowledge text target normalization failed"),
            Self::References(_) => f.write_str("Knowledge Markdown references failed"),
            Self::Communities(_) => f.write_str("Knowledge Markdown communities failed"),
            Self::Serialization(_) => f.write_str("Knowledge Markdown serialization failed"),
            Self::Metadata(_) => f.write_str("Knowledge text metadata serialization failed"),
            Self::InvalidOkfMetadata(field) => {
                write!(f, "Knowledge OKF metadata unavailable: {field}")
            }
            Self::Payload(_) => f.write_str("Knowledge Markdown payload validation failed"),
            Self::NoDocuments => f.write_str("Knowledge Markdown requires published documents"),
            Self::InvalidDisplayText => f.write_str("Knowledge Markdown display text contains NUL"),
            Self::Limit(label) => write!(f, "Knowledge Markdown exceeded {label}"),
            Self::Overflow(label) => write!(f, "Knowledge Markdown overflowed {label}"),
            Self::Allocation(_) => f.write_str("Knowledge Markdown allocation failed"),
        }
    }
}
impl Error for MarkdownExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) | Self::Payload(e) => Some(e),
            Self::Source(e) => Some(e),
            Self::Parse(e) => Some(e),
            Self::Target(e) => Some(e.as_ref()),
            Self::References(e) => Some(e),
            Self::Communities(e) => Some(e),
            Self::Serialization(e) => Some(e),
            Self::Metadata(e) => Some(e),
            Self::Allocation(e) => Some(e),
            _ => None,
        }
    }
}
type Result<T> = std::result::Result<T, MarkdownExportError>;
fn check(original: &SnapshotContext) -> Result<()> {
    original.check().map_err(MarkdownExportError::Request)
}
fn add(a: usize, b: usize, label: &'static str) -> Result<usize> {
    a.checked_add(b).ok_or(MarkdownExportError::Overflow(label))
}
fn cap(used: usize, limit: NonZeroUsize, label: &'static str) -> Result<()> {
    if used > limit.get() {
        return Err(MarkdownExportError::Limit(label));
    }
    Ok(())
}
fn reserve<T>(values: &mut Vec<T>, additional: usize) -> Result<()> {
    values
        .try_reserve(additional)
        .map_err(MarkdownExportError::Allocation)
}
fn filename(slug: &str) -> String {
    format!("p-{}.md", hex::encode(Sha256::digest(slug.as_bytes())))
}
fn display<'a>(original: &SnapshotContext, text: &'a str) -> Result<CowStr<'a>> {
    check(original)?;
    let mut changed = false;
    for c in text.chars() {
        check(original)?;
        if c == '\0' {
            return Err(MarkdownExportError::InvalidDisplayText);
        }
        changed |= matches!(c, '\r' | '\n' | '\t');
    }
    if !changed {
        return Ok(text.into());
    }
    let mut flat = String::new();
    flat.try_reserve(text.len())
        .map_err(MarkdownExportError::Allocation)?;
    for c in text.chars() {
        check(original)?;
        flat.push(if matches!(c, '\r' | '\n' | '\t') {
            ' '
        } else {
            c
        });
    }
    Ok(flat.into())
}
fn title(d: &KnowledgeDocument) -> &str {
    if d.title.trim().is_empty() {
        &d.slug
    } else {
        &d.title
    }
}

struct Events<'a> {
    values: Vec<Event<'a>>,
}
impl<'a> Events<'a> {
    fn push(&mut self, original: &SnapshotContext, event: Event<'a>) -> Result<()> {
        check(original)?;
        reserve(&mut self.values, 1)?;
        self.values.push(event);
        Ok(())
    }
    fn heading(
        &mut self,
        original: &SnapshotContext,
        level: HeadingLevel,
        label: &'a str,
    ) -> Result<()> {
        self.push(
            original,
            Event::Start(Tag::Heading {
                level,
                id: None,
                classes: Vec::new(),
                attrs: Vec::new(),
            }),
        )?;
        self.push(original, Event::Text(display(original, label)?))?;
        self.push(original, Event::End(TagEnd::Heading(level)))
    }
    fn link(
        &mut self,
        original: &SnapshotContext,
        label: &'a str,
        destination: String,
    ) -> Result<()> {
        self.push(
            original,
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: destination.into(),
                title: "".into(),
                id: "".into(),
            }),
        )?;
        self.push(original, Event::Text(display(original, label)?))?;
        self.push(original, Event::End(TagEnd::Link))
    }
}

fn serialize<W: Write>(writer: &mut ExportWriter<'_, W>, events: Events<'_>) -> Result<()> {
    let result = pulldown_cmark_to_cmark::cmark(events.values, &mut *writer, Options::empty());
    if let Some(e) = writer.failure.take() {
        return Err(e);
    }
    result.map_err(MarkdownExportError::Serialization)?;
    check(writer.original)
}
fn heading<W: Write>(
    writer: &mut ExportWriter<'_, W>,
    level: HeadingLevel,
    label: &str,
) -> Result<()> {
    let mut events = Events { values: Vec::new() };
    events.heading(writer.original, level, label)?;
    serialize(writer, events)
}
fn navigation<'a, W: Write>(
    writer: &mut ExportWriter<'_, W>,
    label: &str,
    hits: impl Iterator<Item = std::result::Result<ReferenceHit<'a>, ReferenceError>>,
) -> Result<()> {
    let mut started = false;
    for hit in hits {
        let hit = hit.map_err(MarkdownExportError::References)?;
        if !started {
            heading(writer, HeadingLevel::H2, label)?;
            writer.append(b"\n\n")?;
            started = true;
        }
        let mut events = Events { values: Vec::new() };
        events.push(writer.original, Event::Start(Tag::List(None)))?;
        events.push(writer.original, Event::Start(Tag::Item))?;
        match hit.title {
            Some(title) => events.link(
                writer.original,
                if title.trim().is_empty() {
                    hit.slug
                } else {
                    title
                },
                filename(hit.slug),
            )?,
            None => {
                events.push(
                    writer.original,
                    Event::Text(display(writer.original, hit.slug)?),
                )?;
                events.push(writer.original, Event::Text(" (missing)".into()))?;
            }
        }
        events.push(writer.original, Event::End(TagEnd::Item))?;
        events.push(writer.original, Event::End(TagEnd::List(false)))?;
        serialize(writer, events)?;
        writer.append(b"\n")?;
    }
    if started {
        writer.append(b"\n")?;
    }
    Ok(())
}
fn write_index<'a, W: Write>(
    writer: &mut ExportWriter<'_, W>,
    pages: impl Iterator<Item = (&'a str, &'a str)>,
    count: usize,
) -> Result<()> {
    heading(writer, HeadingLevel::H1, "Knowledge index")?;
    writer.append(format!("\n\n{count} pages\n\n").as_bytes())?;
    let mut pages = pages.peekable();
    while let Some((slug, title)) = pages.next() {
        let mut events = Events { values: Vec::new() };
        events.push(writer.original, Event::Start(Tag::List(None)))?;
        events.push(writer.original, Event::Start(Tag::Item))?;
        events.link(
            writer.original,
            if title.trim().is_empty() { slug } else { title },
            format!("pages/{}", filename(slug)),
        )?;
        events.push(writer.original, Event::End(TagEnd::Item))?;
        events.push(writer.original, Event::End(TagEnd::List(false)))?;
        serialize(writer, events)?;
        if pages.peek().is_some() {
            writer.append(b"\n")?;
        }
    }
    Ok(())
}
fn write_header<W: Write>(
    writer: &mut ExportWriter<'_, W>,
    d: &KnowledgeDocument,
    format: &KnowledgeTextExportFormat,
) -> Result<()> {
    let native = native_metadata(writer.original, d).map_err(MarkdownExportError::Source)?;
    match format {
        KnowledgeTextExportFormat::Markdown => write_frontmatter(writer, &native),
        KnowledgeTextExportFormat::Okf { brand }
        | KnowledgeTextExportFormat::Obsidian { brand } => {
            let header = crate::okf_export::metadata(writer.original, brand, d, &native)?;
            write_frontmatter(writer, &header)
        }
    }
}
fn write_frontmatter<W: Write>(
    writer: &mut ExportWriter<'_, W>,
    header: &impl serde::Serialize,
) -> Result<()> {
    writer.append(b"---\n")?;
    let result = write_native_metadata(writer.original, header, writer);
    if let Some(e) = writer.failure.take() {
        return Err(e);
    }
    result.map_err(MarkdownExportError::Source)?;
    if !writer.newline {
        writer.append(b"\n")?;
    }
    writer.append(b"---\n\n")
}
fn copy_checked<W: Write>(reader: &mut impl Read, writer: &mut ExportWriter<'_, W>) -> Result<()> {
    let mut buffer = [0; 8192];
    loop {
        check(writer.original)?;
        let size = reader
            .read(&mut buffer)
            .map_err(MarkdownExportError::Payload)?;
        if size == 0 {
            return Ok(());
        }
        writer.append(&buffer[..size])?;
    }
}

/// Map canonical destinations and preserve both explicit and implicit labels.
/// The producer's resolved targets and the exported roster must both agree.
fn obsidian_body<W: Write>(
    reader: &mut impl Read,
    writer: &mut ExportWriter<'_, W>,
    page: &Page,
    pages: &BTreeMap<String, Page>,
    slug: &str,
    document: usize,
) -> Result<()> {
    let mut input = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        check(writer.original)?;
        let size = reader
            .read(&mut buffer)
            .map_err(MarkdownExportError::Payload)?;
        if size == 0 {
            break;
        }
        if add(input.len(), size, "document bytes")?
            > writer.limits.document.max_document_bytes.get()
        {
            return Err(MarkdownExportError::Parse(
                DocumentParseError::LimitExceeded("document bytes"),
            ));
        }
        reserve(&mut input, size)?;
        input.extend_from_slice(&buffer[..size]);
    }
    let parsed = parse_document(
        writer.original,
        &input,
        &page.fallback_title,
        writer.limits.document,
    )
    .map_err(MarkdownExportError::Parse)?;
    let mut copied = parsed.envelope.body.start;
    for item in &parsed.markdown {
        check(writer.original)?;
        let Event::Start(Tag::Link {
            link_type: LinkType::WikiLink { has_pothole },
            dest_url,
            ..
        }) = &item.event
        else {
            continue;
        };
        let invalid_span = || MarkdownExportError::Parse(DocumentParseError::InvalidSpan);
        let syntax = parsed
            .source
            .get(item.span.clone())
            .ok_or_else(invalid_span)?;
        let Some(inner) = syntax.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) else {
            continue;
        };
        // These are fields of one already recognized token, not a Markdown
        // scanner. Use the producer's canonicalization, never guess an alias.
        let raw_target = inner.split_once('|').map_or(inner, |(target, _)| target);
        if raw_target != dest_url.as_ref() {
            continue;
        }
        let Some(target) = local_target(
            raw_target,
            LinkType::WikiLink {
                has_pothole: *has_pothole,
            },
            slug,
            document,
        )
        .map_err(|error| MarkdownExportError::Target(Box::new(error)))?
        else {
            continue;
        };
        check(writer.original)?;
        let canonical: &str = target.as_ref();
        if !page.targets.iter().any(|known| known == canonical) || !pages.contains_key(canonical) {
            continue;
        }
        let start = add(item.span.start, 2, "link offset")?;
        let end = add(
            start,
            raw_target.find(['?', '#']).unwrap_or(raw_target.len()),
            "link offset",
        )?;
        writer.append(
            parsed
                .source
                .get(copied..start)
                .ok_or_else(invalid_span)?
                .as_bytes(),
        )?;
        writer.append(filename(canonical).as_bytes())?;
        if *has_pothole {
            copied = end;
        } else {
            // The destination used to be the visible label too. Preserve its
            // original spelling explicitly, using the parser's wiki-link form.
            let closing = item.span.end.checked_sub(2).ok_or_else(invalid_span)?;
            writer.append(
                parsed
                    .source
                    .get(end..closing)
                    .ok_or_else(invalid_span)?
                    .as_bytes(),
            )?;
            writer.append(b"|")?;
            writer.append(raw_target.as_bytes())?;
            copied = closing;
        }
    }
    writer.append(
        parsed
            .source
            .get(copied..parsed.envelope.body.end)
            .ok_or(MarkdownExportError::Parse(DocumentParseError::InvalidSpan))?
            .as_bytes(),
    )
}
struct ExportWriter<'a, W> {
    original: &'a SnapshotContext,
    limits: MarkdownExportLimits,
    bytes: W,
    written: usize,
    newline: bool,
    failure: Option<MarkdownExportError>,
}
impl<'a, W: Write> ExportWriter<'a, W> {
    fn new(original: &'a SnapshotContext, limits: MarkdownExportLimits, bytes: W) -> Self {
        Self {
            original,
            limits,
            bytes,
            written: 0,
            newline: false,
            failure: None,
        }
    }
    fn append(&mut self, bytes: &[u8]) -> Result<()> {
        check(self.original)?;
        let size = add(self.written, bytes.len(), "file bytes")?;
        cap(size, self.limits.max_file_bytes, "file bytes")?;
        self.bytes
            .write_all(bytes)
            .map_err(MarkdownExportError::Payload)?;
        self.written = size;
        if let Some(last) = bytes.last() {
            self.newline = *last == b'\n';
        }
        Ok(())
    }
    fn finish(mut self) -> Result<W> {
        if let Some(e) = self.failure.take() {
            return Err(e);
        }
        check(self.original)?;
        self.bytes.flush().map_err(MarkdownExportError::Payload)?;
        check(self.original)?;
        Ok(self.bytes)
    }
    fn record(&mut self, bytes: &[u8]) -> std::result::Result<(), fmt::Error> {
        if self.failure.is_some() {
            return Err(fmt::Error);
        }
        if let Err(e) = self.append(bytes) {
            self.failure = Some(e);
            return Err(fmt::Error);
        }
        Ok(())
    }
}
impl<W: Write> fmt::Write for ExportWriter<'_, W> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.record(text.as_bytes())
    }
}
impl<W: Write> Write for ExportWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.write_all(bytes).map(|()| bytes.len())
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.record(bytes).map_err(|_| match self.failure.as_ref() {
            Some(MarkdownExportError::Request(e)) => e.kind().into(),
            Some(MarkdownExportError::Limit(_)) => io::ErrorKind::FileTooLarge.into(),
            _ => io::ErrorKind::Other.into(),
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        self.write_all(&[])?;
        self.bytes.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{file_lock::Cancellation, text_slice};
    use graphit_data::{
        knowledge_document_projection::{CompilationDigest32, PhysicalSourceDigest32},
        knowledge_index::KnowledgeRecipeParameters,
    };
    use pulldown_cmark::Parser;
    use serde_json::{Value, json};
    use std::time::{Duration, Instant, UNIX_EPOCH};
    fn n(v: usize) -> NonZeroUsize {
        NonZeroUsize::new(v).unwrap()
    }
    fn context() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        )
    }
    fn limits() -> MarkdownExportLimits {
        MarkdownExportLimits {
            max_file_bytes: n(65536),
            document: DocumentParseLimits {
                max_document_bytes: n(65536),
                max_input_bytes: n(131072),
                max_frontmatter_bytes: n(65536),
                max_yaml_events: n(1000),
                max_markdown_events: n(1000),
                max_yaml_depth: n(32),
                max_scalar_bytes: n(65536),
                max_links: n(100),
                max_projection_bytes: n(131072),
                max_summary_bytes: n(32),
            },
            references: ReferenceLimits {
                max_identifier_bytes: n(1000),
            },
        }
    }

    #[test]
    fn staging_is_private_before_document_bytes() -> std::result::Result<(), Box<dyn Error>> {
        use graphit_core::native_document_file::{
            CapturedDocumentDirectory, NativeDocumentFileLimits,
        };
        let original = context();
        let temporary = tempfile::tempdir()?;
        let mut stage = MarkdownExportStage::new_in(
            &original,
            temporary.path(),
            MarkdownExportModes {
                directory: PayloadMode::private_directory(),
                file: PayloadMode::private_file(),
            },
            limits(),
            KnowledgeTextExportFormat::Markdown,
        )?;
        let native = NativeDocumentFileLimits::new(65536, 4096, 256, 65536)?;
        for relative in ["records", "payload", "payload/pages"] {
            let captured = CapturedDocumentDirectory::capture(
                &stage.staging.path().join(relative),
                native,
                &original,
            )?;
            let file = captured.private_handle(&original)?;
            graphit_fs::verify_private_directory(&file)?;
        }
        stage.push(&original, document())?;
        let file = File::open(stage.staging.path().join("records").join(filename("alpha")))?;
        graphit_fs::verify_private_file(&file)?;
        assert!(file.metadata()?.len() > 0);
        Ok(())
    }

    fn render_page(
        original: &SnapshotContext,
        d: &KnowledgeDocument,
        outbound: &[ReferenceHit<'_>],
        inbound: &[ReferenceHit<'_>],
        limits: MarkdownExportLimits,
    ) -> Result<Vec<u8>> {
        let parsed = parse_document(
            original,
            d.body.as_bytes(),
            &d.recipe_parameters.fallback_title,
            limits.document,
        )
        .map_err(MarkdownExportError::Parse)?;
        let mut writer = ExportWriter::new(original, limits, Vec::new());
        write_header(&mut writer, d, &KnowledgeTextExportFormat::Markdown)?;
        heading(&mut writer, HeadingLevel::H1, title(d))?;
        writer.append(b"\n\n")?;
        navigation(&mut writer, "References", outbound.iter().copied().map(Ok))?;
        navigation(&mut writer, "Backlinks", inbound.iter().copied().map(Ok))?;
        heading(&mut writer, HeadingLevel::H2, "Content")?;
        writer.append(b"\n\n")?;
        writer.append(parsed.source[parsed.envelope.body].as_bytes())?;
        writer.finish()
    }
    fn render_index(
        original: &SnapshotContext,
        docs: &[&KnowledgeDocument],
        limits: MarkdownExportLimits,
    ) -> Result<Vec<u8>> {
        let mut writer = ExportWriter::new(original, limits, Vec::new());
        write_index(
            &mut writer,
            docs.iter().map(|d| (d.slug.as_str(), d.title.as_str())),
            docs.len(),
        )?;
        writer.finish()
    }

    fn document() -> KnowledgeDocument {
        KnowledgeDocument { slug:"alpha".into(),title:"Published: [title] <html> &copy; 世界\r\nnext\tline".into(),body:"\u{feff}---\r\ntitle: Original\r\n---\r\n  # Authored\r\n\r\n[[beta]] [link](beta.md)  \r\n".into(),kind:"custom".into(),summary:"editorial preview".into(),description:Some("  Integral\n\nsecond paragraph  ".into()),summary_truncated:true,source_path:"docs/source.md".into(),physical_digest:Some(PhysicalSourceDigest32::from_bytes([17;32])),compilation_digest:CompilationDigest32::from_bytes([42;32]),recipe:"authored-document-v1".into(),recipe_parameters:KnowledgeRecipeParameters{fallback_title:"stored fallback".into(),kind_on_absent:"document".into(),max_summary_bytes:n(32)},updated_at:Some(UNIX_EPOCH-Duration::from_nanos(17)),stale_since:Some(UNIX_EPOCH+Duration::from_nanos(9)),stale_after:Some(Duration::from_secs(60)),resolved_wiki_targets:vec!["beta".into(),"alpha".into(),"beta".into(),"missing".into()] }
    }
    fn header(source: &str) -> (Value, &str) {
        let (yaml, body) = source
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n\n")
            .unwrap();
        (serde_saphyr::from_str(yaml).unwrap(), body)
    }
    fn heading_text(source: &str, level: HeadingLevel) -> String {
        let mut active = false;
        let mut text = String::new();
        for e in Parser::new_ext(source, Options::empty()) {
            match e {
                Event::Start(Tag::Heading { level: l, .. }) if l == level => active = true,
                Event::End(TagEnd::Heading(l)) if l == level && active => return text,
                Event::Text(s) if active => text.push_str(&s),
                Event::Code(_) | Event::InlineHtml(_) | Event::Html(_) if active => {
                    panic!("display label introduced markup")
                }
                _ => {}
            }
        }
        text
    }

    #[test]
    fn native_metadata_roundtrips_and_p1_body_is_literal_final_suffix() {
        let ctx = context();
        let mut d = document();
        for description in [None, Some(String::new()), d.description.clone()] {
            d.description = description;
            let bytes = render_page(&ctx, &d, &[], &[], limits()).unwrap();
            let exported = String::from_utf8(bytes).unwrap();
            let (h, body) = header(&exported);
            let source = crate::document_source::render_document_source(
                &ctx,
                &d,
                &text_slice::Request::default(),
                crate::document_source::DocumentSourceLimits {
                    max_input_bytes: n(65536),
                    max_targets: n(100),
                    max_virtual_bytes: n(65536),
                    max_virtual_lines: n(1000),
                    max_output_bytes: n(65536),
                    max_matches: n(1000),
                },
            )
            .unwrap();
            assert_eq!(h, header(&source.source).0);
            assert_eq!(h.as_object().unwrap().len(), 15);
            assert_eq!(h["description"], json!(d.description));
            assert_eq!(h["summary"], "editorial preview");
            assert_ne!(h["physical_digest"], h["compilation_digest"]);
            assert_eq!(h["updated_at"], "1969-12-31T23:59:59.999999983Z");
            assert_eq!(h["stale_since"], "1970-01-01T00:00:00.000000009Z");
            assert!(body.ends_with("  # Authored\r\n\r\n[[beta]] [link](beta.md)  \r\n"));
            assert!(!body.contains("title: Original"));
            assert!(!body.contains('\u{feff}'));
            assert_eq!(
                heading_text(body, HeadingLevel::H1),
                "Published: [title] <html> &copy; 世界  next line"
            );
            assert!(!exported.contains("private-observation"));
            assert!(!h.as_object().unwrap().contains_key("generated"));
        }
        for literal in [
            "",
            "  leading\r\ntrailing  ",
            "```\nopen fence\n",
            "<div>\nopen html\n",
        ] {
            d.body = literal.into();
            let bytes = render_page(&ctx, &d, &[], &[], limits()).unwrap();
            let source = String::from_utf8(bytes).unwrap();
            let (_, body) = header(&source);
            assert!(body.ends_with(literal));
            assert!(body[..body.len() - literal.len()].ends_with("\n\n"));
        }
    }

    #[test]
    fn reference_navigation_is_real_deduplicated_and_missing_never_links() {
        let ctx = context();
        let a = document();
        let mut b = document();
        b.slug = "beta".into();
        b.title = "][evil](https://evil.invalid) &copy; <b>β</b>".into();
        b.body.clear();
        b.resolved_wiki_targets.clear();
        let pages = [
            PageReferences {
                slug: &a.slug,
                title: &a.title,
                targets: &["beta", "alpha", "beta", "missing"],
            },
            PageReferences {
                slug: &b.slug,
                title: &b.title,
                targets: &[],
            },
        ];
        let graph = ReferenceGraph::build(&ctx, &pages, limits().references).unwrap();
        let outbound = graph
            .neighbors(&ctx, "alpha", Direction::Outbound, 1, n(10))
            .unwrap();
        let inbound = graph
            .neighbors(&ctx, "beta", Direction::Inbound, 1, n(10))
            .unwrap();
        assert_eq!(outbound.len(), 2);
        assert_eq!(inbound.len(), 1);
        let a_bytes = render_page(&ctx, &a, &outbound, &[], limits()).unwrap();
        let a_text = String::from_utf8(a_bytes).unwrap();
        let (_, body) = header(&a_text);
        let prefix =
            &body[..body.len() - "  # Authored\r\n\r\n[[beta]] [link](beta.md)  \r\n".len()];
        let links = Parser::new_ext(prefix, Options::empty())
            .filter_map(|e| match e {
                Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.into_string()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            links,
            ["p-f44e64e75f3948e9f73f8dfa94721c4ce8cbb4f265c4790c702b2d41cfbf2753.md"]
        );
        let text = Parser::new_ext(prefix, Options::empty())
            .filter_map(|e| match e {
                Event::Text(t) => Some(t.into_string()),
                _ => None,
            })
            .collect::<String>();
        assert!(text.contains(&b.title));
        assert!(text.contains("missing (missing)"));
        assert!(!links.iter().any(|l| l.contains("evil")));
        assert!(
            !Parser::new_ext(prefix, Options::empty())
                .any(|e| matches!(e, Event::Html(_) | Event::InlineHtml(_)))
        );
        let b_bytes = render_page(&ctx, &b, &[], &inbound, limits()).unwrap();
        let b_text = String::from_utf8(b_bytes).unwrap();
        let (_, body) = header(&b_text);
        assert!(body.contains("Backlinks"));
        let index = String::from_utf8(render_index(&ctx, &[&a, &b], limits()).unwrap()).unwrap();
        let links = Parser::new_ext(&index, Options::empty())
            .filter_map(|e| match e {
                Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.into_string()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            links,
            [
                "pages/p-8ed3f6ad685b959ead7022518e1af76cd816f8e8ec7ccdda1ed4018e8f2223f8.md",
                "pages/p-f44e64e75f3948e9f73f8dfa94721c4ce8cbb4f265c4790c702b2d41cfbf2753.md"
            ]
        );
        assert!(index.contains("2 pages"));
    }

    #[test]
    fn structured_display_punctuation_entities_unicode_and_fallback_stay_text() {
        let ctx = context();
        let mut d = document();
        d.body.clear();
        for label in [
            "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~",
            "[title](https://evil.invalid) &copy; &#65; <script>x</script> 世界",
            "line\r\nnext\ttab",
        ] {
            d.title = label.into();
            let source =
                String::from_utf8(render_page(&ctx, &d, &[], &[], limits()).unwrap()).unwrap();
            let (_, body) = header(&source);
            assert_eq!(
                heading_text(body, HeadingLevel::H1),
                label.replace(['\r', '\n', '\t'], " ")
            );
        }
        d.title = " \r\n\t".into();
        let source = String::from_utf8(render_page(&ctx, &d, &[], &[], limits()).unwrap()).unwrap();
        let (h, body) = header(&source);
        assert_eq!(h["title"], d.title);
        assert_eq!(heading_text(body, HeadingLevel::H1), "alpha");
    }

    #[test]
    fn whitespace_titles_use_slug_in_real_outbound_and_backlink_labels() {
        let ctx = context();
        let mut a = document();
        a.body.clear();
        a.title = " \r\n\t".into();
        let mut b = document();
        b.slug = "beta".into();
        b.body.clear();
        b.title = "  \n".into();
        let pages = [
            PageReferences {
                slug: &a.slug,
                title: &a.title,
                targets: &["beta"],
            },
            PageReferences {
                slug: &b.slug,
                title: &b.title,
                targets: &[],
            },
        ];
        let graph = ReferenceGraph::build(&ctx, &pages, limits().references).unwrap();
        let outbound = graph
            .neighbors(&ctx, "alpha", Direction::Outbound, 1, n(10))
            .unwrap();
        let inbound = graph
            .neighbors(&ctx, "beta", Direction::Inbound, 1, n(10))
            .unwrap();
        for (d, outbound, inbound, expected) in [
            (&a, outbound.as_slice(), &[][..], "beta"),
            (&b, &[][..], inbound.as_slice(), "alpha"),
        ] {
            let text =
                String::from_utf8(render_page(&ctx, d, outbound, inbound, limits()).unwrap())
                    .unwrap();
            let (_, body) = header(&text);
            let mut active = false;
            let mut label = String::new();
            for event in Parser::new_ext(body, Options::empty()) {
                match event {
                    Event::Start(Tag::Link { .. }) => active = true,
                    Event::End(TagEnd::Link) => active = false,
                    Event::Text(t) if active => label.push_str(&t),
                    _ => {}
                }
            }
            assert_eq!(label, expected);
        }
    }

    #[test]
    fn sdk_fmt_and_yaml_writers_preserve_first_cancel_without_retry() {
        let cancel = Cancellation::default();
        let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
        let mut writer = ExportWriter::new(&ctx, limits(), Vec::new());
        writer.append(b"prefix").unwrap();
        let events = [
            Event::Start(Tag::Paragraph),
            Event::Text("rejected".into()),
            Event::End(TagEnd::Paragraph),
        ]
        .into_iter()
        .inspect(|event| {
            if matches!(event, Event::End(TagEnd::Paragraph)) {
                cancel.cancel();
            }
        });
        assert!(pulldown_cmark_to_cmark::cmark(events, &mut writer, Options::empty()).is_err());
        assert!(
            matches!(writer.failure.as_ref(),Some(MarkdownExportError::Request(e)) if e.kind()==io::ErrorKind::Interrupted)
        );
        assert_eq!(writer.bytes, b"prefix");
        assert!(fmt::Write::write_str(&mut writer, "").is_err());
        assert!(io::Write::write_all(&mut writer, b"").is_err());
        assert!(io::Write::flush(&mut writer).is_err());
        assert!(
            matches!(writer.failure.as_ref(),Some(MarkdownExportError::Request(e)) if e.kind()==io::ErrorKind::Interrupted)
        );
        let d = document();
        assert!(
            write_native_metadata(&ctx, &native_metadata(&context(), &d).unwrap(), &mut writer)
                .is_err()
        );
        assert_eq!(writer.bytes, b"prefix");
    }

    #[test]
    fn arithmetic_sdk_budget_and_late_deadline_are_explicit() {
        assert!(matches!(
            add(usize::MAX, 1, "output bytes"),
            Err(MarkdownExportError::Overflow("output bytes"))
        ));
        let ctx = context();
        let mut l = limits();
        l.max_file_bytes = n(5);
        let mut w = ExportWriter::new(&ctx, l, Vec::new());
        w.append(b"12345").unwrap();
        assert!(fmt::Write::write_str(&mut w, "6").is_err());
        assert!(matches!(
            w.failure,
            Some(MarkdownExportError::Limit("file bytes"))
        ));
        assert_eq!(w.bytes, b"12345");
        let ctx = SnapshotContext::new(Instant::now(), Cancellation::default());
        let mut w = ExportWriter::new(&ctx, limits(), Vec::new());
        assert!(fmt::Write::write_str(&mut w, "").is_err());
        assert!(
            matches!(w.failure,Some(MarkdownExportError::Request(e)) if e.kind()==io::ErrorKind::TimedOut)
        );
    }
}
