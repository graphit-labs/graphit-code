//! Editorial metadata admitted from an existing retained FILE capture.
//!
//! The source borrows Core's external owner. Limits bound admitted text and SDK
//! work; the original context preserves identity, cancellation and deadline.

use crate::document::{
    DocumentParseError, DocumentParseLimits, Metadata, MetadataKind, MetadataNode, parse_document,
};
use graphit_core::{artifact_store::CapturedSourceReader, file_io::SnapshotContext};
use graphit_fs::NodeKind;
use serde::{
    Deserializer,
    de::{self, DeserializeSeed, IgnoredAny, MapAccess, Visitor},
};
use std::{error::Error, fmt, io, num::NonZeroUsize, ops::Range, path::Path, sync::Arc};

/// Explicit caller policy; supplying this data grants no source authority.
#[derive(Debug)]
pub struct PublicationMetadataPolicy {
    pub fallback_title: String,
    pub kind_on_absent: String,
}

#[derive(Clone, Copy, Debug)]
pub struct PublicationMetadataLimits {
    pub max_input_bytes: NonZeroUsize,
    pub max_title_bytes: NonZeroUsize,
    pub max_kind_bytes: NonZeroUsize,
    pub max_description_bytes: NonZeroUsize,
    pub max_effective_summary_bytes: NonZeroUsize,
    pub max_effective_metadata_bytes: NonZeroUsize,
    pub max_owned_metadata_bytes: NonZeroUsize,
    pub max_root_entries: NonZeroUsize,
    pub max_typed_nodes: NonZeroUsize,
    pub max_typed_depth: NonZeroUsize,
    pub max_typed_scalar_bytes: NonZeroUsize,
    pub max_typed_events: NonZeroUsize,
    /// Scanner characters, independently of raw/scalar UTF-8 byte capacities.
    pub max_typed_lookahead_characters: NonZeroUsize,
    pub max_typed_flow_depth: NonZeroUsize,
}

#[derive(Debug, Eq, PartialEq)]
pub struct PhysicalSourceDigest([u8; 32]);
impl PhysicalSourceDigest {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum AdmittedScalarState {
    Text,
    Null,
}
#[derive(Debug, Eq, PartialEq)]
pub enum FieldProvenance {
    Unknown,
    Absent,
    AuthoredScalar {
        span: Range<usize>,
        state: AdmittedScalarState,
    },
    SuppliedKind {
        authored_null_span: Option<Range<usize>>,
    },
}
#[derive(Debug)]
pub struct PublicationProvenance {
    title: FieldProvenance,
    description: FieldProvenance,
    kind: FieldProvenance,
}
impl PublicationProvenance {
    pub fn title(&self) -> &FieldProvenance {
        &self.title
    }
    pub fn description(&self) -> &FieldProvenance {
        &self.description
    }
    pub fn kind(&self) -> &FieldProvenance {
        &self.kind
    }
}

enum EffectiveKind {
    Authored(String),
    SuppliedPolicy,
}

// Editorial buffers use ordinary Rust ownership; the captured body remains
// borrowed from the external reader.
struct MetadataOwner {
    policy: PublicationMetadataPolicy,
    title: String,
    summary: String,
    description: Option<String>,
    kind: EffectiveKind,
}
impl MetadataOwner {
    fn kind(&self) -> &str {
        match &self.kind {
            EffectiveKind::Authored(text) => text,
            EffectiveKind::SuppliedPolicy => &self.policy.kind_on_absent,
        }
    }
    fn owned_bytes(&self) -> Result<usize> {
        total(
            &[
                self.policy.fallback_title.len(),
                self.policy.kind_on_absent.len(),
                self.title.len(),
                self.summary.len(),
                self.description.as_ref().map_or(0, String::len),
                match &self.kind {
                    EffectiveKind::Authored(s) => s.len(),
                    EffectiveKind::SuppliedPolicy => 0,
                },
            ],
            "owned metadata bytes",
        )
    }
    fn effective_bytes(&self) -> Result<usize> {
        total(
            &[
                self.title.len(),
                self.kind().len(),
                self.description.as_ref().map_or(0, String::len),
                self.summary.len(),
            ],
            "effective metadata bytes",
        )
    }
}

/// A factory-only product borrowing its captured reader and admitted source bytes.
/// Neither its digest nor its provenance reconstructs a capture grant.
pub struct CompiledDocument<'a> {
    reader: &'a CapturedSourceReader,
    source: &'a str,
    metadata: MetadataOwner,
    provenance: PublicationProvenance,
    physical_digest: PhysicalSourceDigest,
    summary_truncated: bool,
    summary_limit: u64,
}
impl CompiledDocument<'_> {
    pub fn whole_source(&self) -> &str {
        self.source
    }
    pub fn title(&self) -> &str {
        &self.metadata.title
    }
    pub fn kind(&self) -> &str {
        self.metadata.kind()
    }
    pub fn description(&self) -> Option<&str> {
        self.metadata.description.as_deref()
    }
    pub fn summary(&self) -> &str {
        &self.metadata.summary
    }
    pub fn summary_truncated(&self) -> bool {
        self.summary_truncated
    }
    pub fn physical_source_digest(&self) -> &PhysicalSourceDigest {
        &self.physical_digest
    }
    pub fn provenance(&self) -> &PublicationProvenance {
        &self.provenance
    }
    pub fn recipe_policy(&self) -> &PublicationMetadataPolicy {
        &self.metadata.policy
    }
    pub fn summary_limit(&self) -> u64 {
        self.summary_limit
    }
    pub(crate) fn check_request(&self, request: &Arc<SnapshotContext>) -> io::Result<()> {
        self.reader.check_request(request)
    }
}

#[derive(Debug)]
pub enum CompilationMetadataError {
    OriginalRequest(io::Error),
    PhysicalSource(io::Error),
    Parser(DocumentParseError),
    TypedMetadata(serde_saphyr::Error),
    MissingCapturedFile,
    NonFile,
    UnretainedFile,
    MissingPhysicalDigest,
    MetadataMismatch,
    Limit(&'static str),
    Overflow(&'static str),
}
impl fmt::Display for CompilationMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OriginalRequest(_) => {
                f.write_str("original capture request denied metadata compilation")
            }
            Self::PhysicalSource(_) => {
                f.write_str("captured file bytes failed physical validation")
            }
            Self::Parser(_) => f.write_str("document syntax denied metadata compilation"),
            Self::TypedMetadata(_) => f.write_str("typed editorial metadata is invalid"),
            Self::MissingCapturedFile => f.write_str("file is not in the captured plan"),
            Self::NonFile => f.write_str("captured node is not a file"),
            Self::UnretainedFile => f.write_str("captured file bytes are not retained"),
            Self::MissingPhysicalDigest => f.write_str("captured file digest is missing"),
            Self::MetadataMismatch => f.write_str("typed metadata does not match captured syntax"),
            Self::Limit(name) => write!(f, "metadata limit exceeded: {name}"),
            Self::Overflow(name) => write!(f, "metadata arithmetic overflow: {name}"),
        }
    }
}
impl Error for CompilationMetadataError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::OriginalRequest(error) => Some(error),
            Self::PhysicalSource(error) => Some(error),
            Self::Parser(error) => Some(error),
            Self::TypedMetadata(error) => Some(error),
            _ => None,
        }
    }
}
impl From<io::Error> for CompilationMetadataError {
    fn from(error: io::Error) -> Self {
        Self::OriginalRequest(error)
    }
}
type Result<T> = std::result::Result<T, CompilationMetadataError>;

fn total(amounts: &[usize], name: &'static str) -> Result<usize> {
    amounts.iter().try_fold(0usize, |sum, &n| {
        sum.checked_add(n)
            .ok_or(CompilationMetadataError::Overflow(name))
    })
}
fn admit(bytes: usize, limit: NonZeroUsize, name: &'static str) -> Result<()> {
    if bytes > limit.get() {
        Err(CompilationMetadataError::Limit(name))
    } else {
        Ok(())
    }
}

pub fn compile_captured_document<'a>(
    reader: &'a CapturedSourceReader,
    relative: &Path,
    request: &Arc<SnapshotContext>,
    policy: PublicationMetadataPolicy,
    parse_limits: DocumentParseLimits,
    limits: PublicationMetadataLimits,
) -> Result<CompiledDocument<'a>> {
    finish_candidate(
        prepare_candidate(reader, relative, request, policy, parse_limits, limits)?,
        request,
    )
}

/// Compile one externally owned FILE buffer validated against its captured reader.
/// The result borrows both owners; neither the digest nor this product grants access.
///
/// A local body cannot escape its owner:
/// ```compile_fail
/// # use graphit_core::{artifact_store::CapturedSourceReader, file_io::SnapshotContext};
/// # use graphit_knowledge::{document::DocumentParseLimits, publication_metadata::*};
/// # use std::{path::Path, sync::Arc};
/// fn local_buffer<'a>(reader: &'a CapturedSourceReader, request: &Arc<SnapshotContext>,
///     policy: PublicationMetadataPolicy, parser: DocumentParseLimits,
///     limits: PublicationMetadataLimits) -> CompiledDocument<'a> {
///     let bytes = b"# Guide".to_vec();
///     compile_captured_file_bytes(reader, Path::new("guide.md"), &bytes, request,
///         policy, parser, limits).unwrap()
/// }
/// ```
/// The captured reader must also outlive the result:
/// ```compile_fail
/// # use graphit_core::{artifact_store::CapturedSourceReader, file_io::SnapshotContext};
/// # use graphit_knowledge::{document::DocumentParseLimits, publication_metadata::*};
/// # use std::{path::Path, sync::Arc};
/// fn local_reader<'a>(reader: CapturedSourceReader, bytes: &'a [u8], request: &Arc<SnapshotContext>,
///     policy: PublicationMetadataPolicy, parser: DocumentParseLimits,
///     limits: PublicationMetadataLimits) -> CompiledDocument<'a> {
///     compile_captured_file_bytes(&reader, Path::new("guide.md"), bytes, request,
///         policy, parser, limits).unwrap()
/// }
/// ```
/// Borrowing both owners for the returned lifetime is valid:
/// ```no_run
/// # use graphit_core::{artifact_store::CapturedSourceReader, file_io::SnapshotContext};
/// # use graphit_knowledge::{document::DocumentParseLimits, publication_metadata::*};
/// # use std::{path::Path, sync::Arc};
/// fn borrow_owners<'a>(reader: &'a CapturedSourceReader, bytes: &'a [u8], request: &Arc<SnapshotContext>,
///     policy: PublicationMetadataPolicy, parser: DocumentParseLimits,
///     limits: PublicationMetadataLimits) -> Result<CompiledDocument<'a>, CompilationMetadataError> {
///     compile_captured_file_bytes(reader, Path::new("guide.md"), bytes, request, policy, parser, limits)
/// }
/// ```
pub fn compile_captured_file_bytes<'a>(
    reader: &'a CapturedSourceReader,
    relative: &Path,
    bytes: &'a [u8],
    request: &Arc<SnapshotContext>,
    policy: PublicationMetadataPolicy,
    parse_limits: DocumentParseLimits,
    limits: PublicationMetadataLimits,
) -> Result<CompiledDocument<'a>> {
    let digest = captured_file_digest(reader, relative, request)?;
    reader
        .validate_file_bytes(relative, bytes, request)
        .map_err(CompilationMetadataError::PhysicalSource)?;
    finish_candidate(
        prepare_bytes_candidate(reader, bytes, digest, request, policy, parse_limits, limits)?,
        request,
    )
}

// Production preparation and completion are separate private boundaries so a
// final cancellation can be exercised after actual capture/parser/SDK work.
fn prepare_candidate<'a>(
    reader: &'a CapturedSourceReader,
    relative: &Path,
    request: &Arc<SnapshotContext>,
    policy: PublicationMetadataPolicy,
    parse_limits: DocumentParseLimits,
    limits: PublicationMetadataLimits,
) -> Result<CompiledDocument<'a>> {
    let digest = captured_file_digest(reader, relative, request)?;
    let bytes = reader
        .retained(relative)
        .ok_or(CompilationMetadataError::UnretainedFile)?;
    prepare_bytes_candidate(reader, bytes, digest, request, policy, parse_limits, limits)
}

fn captured_file_digest(
    reader: &CapturedSourceReader,
    relative: &Path,
    request: &Arc<SnapshotContext>,
) -> Result<[u8; 32]> {
    reader.check_request(request)?;
    let nodes = reader.nodes();
    let mut digest = None;
    for node in nodes {
        reader.check_request(request)?;
        if node.relative() == relative {
            if node.kind() != NodeKind::File {
                return Err(CompilationMetadataError::NonFile);
            }
            digest = Some(
                node.digest()
                    .ok_or(CompilationMetadataError::MissingPhysicalDigest)?,
            );
            break;
        }
    }
    digest.ok_or(CompilationMetadataError::MissingCapturedFile)
}

fn prepare_bytes_candidate<'a>(
    reader: &'a CapturedSourceReader,
    bytes: &'a [u8],
    digest: [u8; 32],
    request: &Arc<SnapshotContext>,
    policy: PublicationMetadataPolicy,
    parse_limits: DocumentParseLimits,
    limits: PublicationMetadataLimits,
) -> Result<CompiledDocument<'a>> {
    reader.check_request(request)?;
    admit(
        total(
            &[
                bytes.len(),
                policy.fallback_title.len(),
                policy.kind_on_absent.len(),
            ],
            "input bytes",
        )?,
        limits.max_input_bytes,
        "input bytes",
    )?;
    let policy_bytes = total(
        &[policy.fallback_title.len(), policy.kind_on_absent.len()],
        "owned metadata bytes",
    )?;
    admit(
        policy_bytes,
        limits.max_owned_metadata_bytes,
        "owned metadata bytes",
    )?;
    let summary_limit = u64::try_from(parse_limits.max_summary_bytes.get())
        .map_err(|_| CompilationMetadataError::Overflow("summary recipe limit"))?;
    let mut metadata = MetadataOwner {
        policy,
        title: String::new(),
        summary: String::new(),
        description: None,
        kind: EffectiveKind::SuppliedPolicy,
    };
    let mut provenance = PublicationProvenance {
        title: FieldProvenance::Unknown,
        description: FieldProvenance::Absent,
        kind: FieldProvenance::SuppliedKind {
            authored_null_span: None,
        },
    };
    let source;
    let summary_truncated;
    {
        reader.check_request(request)?;
        let mut parsed = parse_document(
            request,
            bytes,
            &metadata.policy.fallback_title,
            parse_limits,
        )
        .map_err(CompilationMetadataError::Parser)?;
        source = parsed.source;
        summary_truncated = parsed.summary_truncated;
        admit(parsed.title.len(), limits.max_title_bytes, "title bytes")?;
        admit(
            parsed.summary.len(),
            limits.max_effective_summary_bytes,
            "effective summary bytes",
        )?;
        admit(
            total(
                &[
                    metadata.owned_bytes()?,
                    parsed.title.len(),
                    parsed.summary.len(),
                ],
                "owned metadata bytes",
            )?,
            limits.max_owned_metadata_bytes,
            "owned metadata bytes",
        )?;
        admit(
            total(
                &[parsed.title.len(), parsed.summary.len()],
                "effective metadata bytes",
            )?,
            limits.max_effective_metadata_bytes,
            "effective metadata bytes",
        )?;
        metadata.title = std::mem::take(&mut parsed.title);
        metadata.summary = std::mem::take(&mut parsed.summary);
        if let Some(root) = parsed.metadata.root {
            let root = parsed
                .metadata
                .node(root)
                .ok_or(CompilationMetadataError::MetadataMismatch)?;
            let MetadataKind::Mapping { entries } = &root.kind else {
                return Err(CompilationMetadataError::MetadataMismatch);
            };
            admit(entries.len(), limits.max_root_entries, "root entries")?;
            let description =
                known_node(&parsed.metadata, entries, "description", reader, request)?;
            let kind = known_node(&parsed.metadata, entries, "type", reader, request)?;
            let range = parsed
                .envelope
                .yaml
                .as_ref()
                .ok_or(CompilationMetadataError::MetadataMismatch)?;
            let yaml = parsed
                .source
                .get(range.clone())
                .ok_or(CompilationMetadataError::MetadataMismatch)?;
            reader.check_request(request)?;
            let mut local_failure = None;
            let mut state = DecodeState {
                reader,
                request,
                metadata: &mut metadata,
                provenance: &mut provenance,
                description,
                kind,
                limits,
                local_failure: &mut local_failure,
                kind_resolved: kind.is_none(),
            };
            decode_mapping(yaml, &mut state)?;
            reader.check_request(request)?;
        }
        admit(metadata.kind().len(), limits.max_kind_bytes, "kind bytes")?;
        admit(
            metadata.effective_bytes()?,
            limits.max_effective_metadata_bytes,
            "effective metadata bytes",
        )?;
        admit(
            metadata.owned_bytes()?,
            limits.max_owned_metadata_bytes,
            "owned metadata bytes",
        )?;
        drop(parsed);
    }
    Ok(CompiledDocument {
        reader,
        source,
        metadata,
        provenance,
        physical_digest: PhysicalSourceDigest(digest),
        summary_truncated,
        summary_limit,
    })
}
fn finish_candidate<'a>(
    candidate: CompiledDocument<'a>,
    request: &Arc<SnapshotContext>,
) -> Result<CompiledDocument<'a>> {
    candidate.check_request(request)?;
    Ok(candidate)
}

fn known_node<'m, 'a>(
    metadata: &'m Metadata<'a>,
    entries: &[(crate::document::NodeId, crate::document::NodeId)],
    name: &str,
    reader: &CapturedSourceReader,
    request: &Arc<SnapshotContext>,
) -> Result<Option<&'m MetadataNode<'a>>> {
    for &(key, value) in entries {
        reader.check_request(request)?;
        let key = metadata
            .node(key)
            .ok_or(CompilationMetadataError::MetadataMismatch)?;
        if let MetadataKind::Scalar { value: key, .. } = &key.kind
            && key == name
        {
            return metadata
                .node(value)
                .map(Some)
                .ok_or(CompilationMetadataError::MetadataMismatch);
        }
    }
    Ok(None)
}
fn typed_options(limits: PublicationMetadataLimits) -> serde_saphyr::Options {
    let budget = serde_saphyr::budget! {
        max_reader_input_bytes: None,
        max_buffered_comment_events: 0,
        simple_key_max_lookahead: limits.max_typed_lookahead_characters.get(),
        flow_nesting_limit: limits.max_typed_flow_depth.get(),
        max_events: limits.max_typed_events.get(),
        max_aliases: 0, max_anchors: 0, max_recorded_anchor_events: 0,
        max_recorded_anchor_bytes: 0, max_property_expansion_depth: 0,
        max_total_property_interpolation_work: 0,
        max_depth: limits.max_typed_depth.get(), max_inclusion_depth: 0,
        max_documents: 1, max_nodes: limits.max_typed_nodes.get(),
        max_total_scalar_bytes: limits.max_typed_scalar_bytes.get(),
        max_total_comment_bytes: 0, max_merge_keys: 0,
    };
    serde_saphyr::options! {
        budget: budget, no_schema: true, strict_booleans: true,
        legacy_octal_numbers: false,
        merge_keys: serde_saphyr::options::MergeKeyPolicy::Error,
        duplicate_keys: serde_saphyr::options::DuplicateKeyPolicy::FirstWins,
        emit_comments: false, with_snippet: false, reject_unsupported_tags: true,
        reject_non_finite_typeless_float: false,
    }
}

// The factory and focused boundary tests use the same synchronous SDK call.
fn decode_mapping(yaml: &str, state: &mut DecodeState<'_, '_, '_>) -> Result<()> {
    let decoded = serde_saphyr::with_deserializer_from_str_with_options(
        yaml,
        typed_options(state.limits),
        |deserializer| RootSeed(&mut *state).deserialize(deserializer),
    );
    // Serde custom errors stringify causes. A first local error always wins,
    // even if the SDK later succeeds; vendor Display text is never classified.
    if let Some(error) = state.local_failure.take() {
        return Err(error);
    }
    decoded.map_err(CompilationMetadataError::TypedMetadata)
}

#[derive(Clone, Copy)]
enum Field {
    Description,
    Kind,
    Ignored,
}
struct KeyVisitor;
impl<'de> Visitor<'de> for KeyVisitor {
    type Value = Field;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an admitted primitive mapping key")
    }
    fn visit_str<E: de::Error>(self, s: &str) -> std::result::Result<Field, E> {
        Ok(match s {
            "description" => Field::Description,
            "type" => Field::Kind,
            _ => Field::Ignored,
        })
    }
    fn visit_borrowed_str<E: de::Error>(self, s: &'de str) -> std::result::Result<Field, E> {
        self.visit_str(s)
    }
    fn visit_string<E: de::Error>(self, s: String) -> std::result::Result<Field, E> {
        self.visit_str(&s)
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Field, E> {
        Ok(Field::Ignored)
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<Field, E> {
        Ok(Field::Ignored)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<Field, E> {
        Ok(Field::Ignored)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<Field, E> {
        Ok(Field::Ignored)
    }
    fn visit_i128<E: de::Error>(self, _: i128) -> std::result::Result<Field, E> {
        Ok(Field::Ignored)
    }
    fn visit_u128<E: de::Error>(self, _: u128) -> std::result::Result<Field, E> {
        Ok(Field::Ignored)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<Field, E> {
        Ok(Field::Ignored)
    }
}
struct KeySeed;
impl<'de> DeserializeSeed<'de> for KeySeed {
    type Value = Field;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<Field, D::Error> {
        d.deserialize_any(KeyVisitor)
    }
}

struct DecodeState<'s, 'a, 'm> {
    reader: &'a CapturedSourceReader,
    request: &'s Arc<SnapshotContext>,
    metadata: &'s mut MetadataOwner,
    provenance: &'s mut PublicationProvenance,
    description: Option<&'m MetadataNode<'a>>,
    kind: Option<&'m MetadataNode<'a>>,
    limits: PublicationMetadataLimits,
    local_failure: &'s mut Option<CompilationMetadataError>,
    kind_resolved: bool,
}
impl DecodeState<'_, '_, '_> {
    fn preserve<T, E: de::Error>(&mut self, result: Result<T>) -> std::result::Result<T, E> {
        result.map_err(|error| {
            if self.local_failure.is_none() {
                *self.local_failure = Some(error);
            }
            E::custom("metadata admission failed")
        })
    }
    fn step<E: de::Error>(&mut self) -> std::result::Result<(), E> {
        let result = self
            .reader
            .check_request(self.request)
            .map_err(CompilationMetadataError::OriginalRequest);
        self.preserve(result)
    }
    fn node(&self, field: Field) -> Result<&MetadataNode<'_>> {
        match field {
            Field::Description => self.description,
            Field::Kind => self.kind,
            Field::Ignored => None,
        }
        .ok_or(CompilationMetadataError::MetadataMismatch)
    }
    fn accept_null<E: de::Error>(&mut self, field: Field) -> std::result::Result<(), E> {
        self.step()?;
        let result = self.node(field).and_then(|node| {
            if matches!(node.kind, MetadataKind::Scalar { .. }) {
                Ok(node.span.clone())
            } else {
                Err(CompilationMetadataError::MetadataMismatch)
            }
        });
        let span = self.preserve(result)?;
        match field {
            Field::Description => {
                self.provenance.description = FieldProvenance::AuthoredScalar {
                    span,
                    state: AdmittedScalarState::Null,
                }
            }
            Field::Kind => {
                let result = self.metadata.effective_bytes().and_then(|bytes| {
                    admit(
                        bytes,
                        self.limits.max_effective_metadata_bytes,
                        "effective metadata bytes",
                    )
                });
                self.preserve(result)?;
                self.kind_resolved = true;
                self.provenance.kind = FieldProvenance::SuppliedKind {
                    authored_null_span: Some(span),
                }
            }
            Field::Ignored => unreachable!(),
        }
        Ok(())
    }
    fn admit_text(&self, field: Field, text: &str) -> Result<Range<usize>> {
        let node = self.node(field)?;
        let MetadataKind::Scalar { value, .. } = &node.kind else {
            return Err(CompilationMetadataError::MetadataMismatch);
        };
        if value != text {
            return Err(CompilationMetadataError::MetadataMismatch);
        }
        let (limit, name) = match field {
            Field::Description => (self.limits.max_description_bytes, "description bytes"),
            Field::Kind => (self.limits.max_kind_bytes, "kind bytes"),
            Field::Ignored => unreachable!(),
        };
        admit(text.len(), limit, name)?;
        admit(
            total(
                &[self.metadata.owned_bytes()?, text.len()],
                "owned metadata bytes",
            )?,
            self.limits.max_owned_metadata_bytes,
            "owned metadata bytes",
        )?;
        let description_bytes = match field {
            Field::Description => text.len(),
            _ => self.metadata.description.as_ref().map_or(0, String::len),
        };
        let kind_bytes = match field {
            Field::Kind => text.len(),
            _ if self.kind_resolved => self.metadata.kind().len(),
            _ => 0,
        };
        admit(
            total(
                &[
                    self.metadata.title.len(),
                    self.metadata.summary.len(),
                    description_bytes,
                    kind_bytes,
                ],
                "effective metadata bytes",
            )?,
            self.limits.max_effective_metadata_bytes,
            "effective metadata bytes",
        )?;
        Ok(node.span.clone())
    }
    fn store_text(&mut self, field: Field, text: String, span: Range<usize>) {
        let provenance = FieldProvenance::AuthoredScalar {
            span,
            state: AdmittedScalarState::Text,
        };
        match field {
            Field::Description => {
                self.metadata.description = Some(text);
                self.provenance.description = provenance;
            }
            Field::Kind => {
                self.metadata.kind = EffectiveKind::Authored(text);
                self.kind_resolved = true;
                self.provenance.kind = provenance;
            }
            Field::Ignored => unreachable!(),
        }
    }
    fn accept_borrowed<E: de::Error>(
        &mut self,
        field: Field,
        text: &str,
    ) -> std::result::Result<(), E> {
        self.step()?;
        let result = self.admit_text(field, text);
        let span = self.preserve(result)?;
        let mut owned = String::new();
        let result = owned.try_reserve_exact(text.len()).map_err(|error| {
            CompilationMetadataError::OriginalRequest(io::Error::new(
                io::ErrorKind::OutOfMemory,
                error,
            ))
        });
        self.preserve(result)?;
        owned.push_str(text);
        self.store_text(field, owned, span);
        Ok(())
    }
    fn accept_owned<E: de::Error>(
        &mut self,
        field: Field,
        text: String,
    ) -> std::result::Result<(), E> {
        self.step()?;
        let result = self.admit_text(field, &text);
        let span = self.preserve(result)?;
        self.store_text(field, text, span);
        Ok(())
    }
}
struct RootSeed<'v, 's, 'a, 'm>(&'v mut DecodeState<'s, 'a, 'm>);
impl<'de> DeserializeSeed<'de> for RootSeed<'_, '_, '_, '_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_map(RootVisitor(self.0))
    }
}
struct RootVisitor<'v, 's, 'a, 'm>(&'v mut DecodeState<'s, 'a, 'm>);
impl<'de> Visitor<'de> for RootVisitor<'_, '_, '_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the admitted root metadata mapping")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<(), A::Error> {
        let mut entries = 0usize;
        while let Some(field) = map.next_key_seed(KeySeed)? {
            self.0.step()?;
            let result = entries
                .checked_add(1)
                .ok_or(CompilationMetadataError::Overflow("root entries"))
                .and_then(|count| {
                    admit(count, self.0.limits.max_root_entries, "root entries")?;
                    Ok(count)
                });
            entries = self.0.preserve(result)?;
            match field {
                Field::Ignored => {
                    map.next_value::<IgnoredAny>()?;
                }
                Field::Description | Field::Kind => {
                    map.next_value_seed(OptionalTextSeed {
                        state: &mut *self.0,
                        field,
                    })?;
                }
            }
        }
        Ok(())
    }
}
struct OptionalTextSeed<'v, 's, 'a, 'm> {
    state: &'v mut DecodeState<'s, 'a, 'm>,
    field: Field,
}
impl<'de> DeserializeSeed<'de> for OptionalTextSeed<'_, '_, '_, '_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_option(OptionalTextVisitor(self))
    }
}
struct OptionalTextVisitor<'v, 's, 'a, 'm>(OptionalTextSeed<'v, 's, 'a, 'm>);
impl<'de> Visitor<'de> for OptionalTextVisitor<'_, '_, '_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("null or editorial text")
    }
    fn visit_none<E: de::Error>(self) -> std::result::Result<(), E> {
        self.0.state.accept_null(self.0.field)
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<(), E> {
        self.0.state.accept_null(self.0.field)
    }
    fn visit_some<D: Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_string(TextVisitor(self.0))
    }
}
struct TextVisitor<'v, 's, 'a, 'm>(OptionalTextSeed<'v, 's, 'a, 'm>);
impl<'de> Visitor<'de> for TextVisitor<'_, '_, '_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("editorial text under the strict SDK string boundary")
    }
    fn visit_str<E: de::Error>(self, text: &str) -> std::result::Result<(), E> {
        self.0.state.accept_borrowed(self.0.field, text)
    }
    fn visit_borrowed_str<E: de::Error>(self, text: &'de str) -> std::result::Result<(), E> {
        self.0.state.accept_borrowed(self.0.field, text)
    }
    fn visit_string<E: de::Error>(self, text: String) -> std::result::Result<(), E> {
        self.0.state.accept_owned(self.0.field, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{
        artifact_store::{
            CapturedPublicationScope, CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet,
            ScopeLimits, SnapshotPolicy, SourceReadLimits,
        },
        brand::Brand,
        file_io::SnapshotContext,
        file_lock::Cancellation,
        project_paths::{InstallationPaths, ProjectPaths},
    };
    use std::{
        fs,
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };
    struct TemporaryDirectory(PathBuf);
    impl TemporaryDirectory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            for _ in 0..16 {
                let nonce = NEXT
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                    .expect("fixture nonce exhausted");
                let path = std::env::temp_dir().join(format!(
                    "graphit-knowledge-project-{}-{nonce}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("create fixture directory: {error}"),
                }
            }
            panic!("fixture directory collision limit exceeded");
        }
    }
    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            if let Err(error) = fs::remove_dir_all(&self.0) {
                if std::thread::panicking() {
                    eprintln!("fixture cleanup during unwinding failed: {error}");
                } else {
                    panic!("fixture cleanup failed: {error}");
                }
            }
        }
    }

    fn request() -> Arc<SnapshotContext> {
        let context = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        Arc::new(context)
    }

    struct Fixture {
        // Fields drop in declaration order: native owners before directory cleanup.
        scope: CapturedPublicationScope,
        request: Arc<SnapshotContext>,
        _paths: ProjectPaths,
        document_bytes: usize,
        _temporary: TemporaryDirectory,
    }
    impl Fixture {
        fn with_document(document: &[u8]) -> Self {
            let temporary = TemporaryDirectory::new();
            let root = temporary.0.join("project");
            fs::create_dir_all(root.join(".graphit")).unwrap();
            fs::write(root.join("document.md"), document).unwrap();
            // A mistaken directory capture exceeds the selected FILE content bounds.
            fs::write(root.join("unselected.bin"), [0; 4096]).unwrap();
            let installation = InstallationPaths::resolve(
                Brand::default(),
                &temporary.0,
                None,
                Some(&temporary.0.join("global")),
            )
            .unwrap();
            let paths = ProjectPaths::at(&root, &installation).unwrap();
            let request = request();
            let scope = CapturedPublicationScope::project(
                &paths,
                "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
                ScopeLimits::new(4, 4096, 256).unwrap(),
                &request,
            )
            .unwrap();
            Self {
                scope,
                request,
                _paths: paths,
                document_bytes: document.len(),
                _temporary: temporary,
            }
        }
    }

    fn capture(
        fixture: &Fixture,
        request: &Arc<SnapshotContext>,
        retention_bytes: usize,
    ) -> io::Result<CapturedSourceReader> {
        let retention = RetainedBytesLimits::new(retention_bytes, 1, 1)?;
        let length = u64::try_from(fixture.document_bytes.max(1)).unwrap();
        let policy = SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            CopyLimits::new(length, 1, 1, 1)?,
            RetainedFileSet::new([PathBuf::new()], retention)?,
            retention,
        )?;
        CapturedSourceReader::capture_project(
            &fixture.scope,
            Path::new("document.md"),
            NodeKind::File,
            &policy,
            SourceReadLimits::new(fixture.document_bytes.max(1), 4096, length)?,
            request,
        )
    }

    fn parse_limits(document_bytes: usize) -> DocumentParseLimits {
        let cap = |n| NonZeroUsize::new(n).unwrap();
        DocumentParseLimits {
            max_document_bytes: cap(document_bytes),
            max_input_bytes: cap(1024),
            max_frontmatter_bytes: cap(1024),
            max_yaml_events: cap(100),
            max_markdown_events: cap(100),
            max_yaml_depth: cap(10),
            max_scalar_bytes: cap(1024),
            max_links: cap(10),
            max_projection_bytes: cap(10_000),
            max_summary_bytes: cap(1024),
        }
    }

    fn limits() -> PublicationMetadataLimits {
        let cap = |n| NonZeroUsize::new(n).unwrap();
        PublicationMetadataLimits {
            max_input_bytes: cap(4096),
            max_title_bytes: cap(1024),
            max_kind_bytes: cap(1024),
            max_description_bytes: cap(1024),
            max_effective_summary_bytes: cap(1024),
            max_effective_metadata_bytes: cap(4096),
            max_owned_metadata_bytes: cap(4096),
            max_root_entries: cap(100),
            max_typed_nodes: cap(100),
            max_typed_depth: cap(10),
            max_typed_scalar_bytes: cap(4096),
            max_typed_events: cap(200),
            max_typed_lookahead_characters: cap(1024),
            max_typed_flow_depth: cap(10),
        }
    }
    fn policy() -> PublicationMetadataPolicy {
        PublicationMetadataPolicy {
            fallback_title: "F".into(),
            kind_on_absent: "guide".into(),
        }
    }

    #[test]
    fn final_original_cancellation_discards_real_compiled_candidate_without_returning_partial() {
        let source = "---\ndescription: α\ntype: custom\n---\n# H\n";
        let fixture = Fixture::with_document(source.as_bytes());
        let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
        let candidate = prepare_candidate(
            &reader,
            Path::new(""),
            &fixture.request,
            policy(),
            parse_limits(source.len()),
            limits(),
        )
        .unwrap();
        assert_eq!(
            (candidate.title(), candidate.description(), candidate.kind()),
            ("H", Some("α"), "custom")
        );
        assert_eq!(
            candidate.whole_source().as_ptr(),
            reader.retained(Path::new("")).unwrap().as_ptr()
        );
        fixture.request.cancellation().cancel();
        let error = finish_candidate(candidate, &fixture.request).err().unwrap();
        assert!(
            matches!(&error,CompilationMetadataError::OriginalRequest(e) if e.kind()==io::ErrorKind::Interrupted)
        );
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(reader.retained(Path::new("")).unwrap(), source.as_bytes());
    }

    #[test]
    fn final_foreign_binding_rejection_suppresses_candidate() {
        let source = "---\ndescription: α\n---\n# H\n";
        let fixture = Fixture::with_document(source.as_bytes());
        let reader = capture(&fixture, &fixture.request, source.len()).unwrap();

        let candidate = prepare_candidate(
            &reader,
            Path::new(""),
            &fixture.request,
            policy(),
            parse_limits(source.len()),
            limits(),
        )
        .unwrap();
        let foreign = request();
        let error = finish_candidate(candidate, &foreign).err().unwrap();
        assert!(
            matches!(&error,CompilationMetadataError::OriginalRequest(e) if e.kind()==io::ErrorKind::InvalidInput)
        );
        assert!(error.source().is_some());
    }

    #[test]
    fn real_sdk_callback_cancellation_keeps_original_io_error_across_serde_custom_boundary() {
        let source = "---\ndescription: α\ntype: custom\n---\n# H\n";
        let fixture = Fixture::with_document(source.as_bytes());
        let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
        let mut candidate = prepare_candidate(
            &reader,
            Path::new(""),
            &fixture.request,
            policy(),
            parse_limits(source.len()),
            limits(),
        )
        .unwrap();
        let parsed = parse_document(
            &fixture.request,
            reader.retained(Path::new("")).unwrap(),
            &candidate.recipe_policy().fallback_title,
            parse_limits(source.len()),
        )
        .unwrap();
        let root = parsed.metadata.node(parsed.metadata.root.unwrap()).unwrap();
        let MetadataKind::Mapping { entries } = &root.kind else {
            panic!("actual parsed mapping required")
        };
        let description = known_node(
            &parsed.metadata,
            entries,
            "description",
            &reader,
            &fixture.request,
        )
        .unwrap();
        let kind =
            known_node(&parsed.metadata, entries, "type", &reader, &fixture.request).unwrap();
        let mut local_failure = None;
        let mut state = DecodeState {
            reader: &reader,
            request: &fixture.request,
            metadata: &mut candidate.metadata,
            provenance: &mut candidate.provenance,
            description,
            kind,
            limits: limits(),
            local_failure: &mut local_failure,
            kind_resolved: false,
        };
        fixture.request.cancellation().cancel();
        let error = decode_mapping(
            &parsed.source[parsed.envelope.yaml.clone().unwrap()],
            &mut state,
        )
        .unwrap_err();
        assert!(
            matches!(&error,CompilationMetadataError::OriginalRequest(e) if e.kind()==io::ErrorKind::Interrupted)
        );
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(reader.retained(Path::new("")).unwrap(), source.as_bytes());
    }
}
