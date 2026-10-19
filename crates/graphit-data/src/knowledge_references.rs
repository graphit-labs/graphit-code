//! Data-only reference witnesses and the closed native Knowledge row extension.
//! These values never grant access or select another SDK generation.
use crate::knowledge_document_projection::{
    DocumentHeader, MemberRow, PreparedKnowledgeDocumentRows,
};
use crate::knowledge_index::{KnowledgeIndexContentLimits, KnowledgeIndexError};
use arrow_array::{
    Array, ArrayRef, FixedSizeBinaryArray, ListArray, StringArray, StructArray, UInt8Array,
    builder::{FixedSizeBinaryBuilder, ListBuilder, StringBuilder, StructBuilder, UInt8Builder},
};
use arrow_schema::{DataType, Field, Fields};
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use serde::{
    Serialize, Serializer,
    ser::{SerializeSeq, SerializeTuple},
};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    num::NonZeroUsize,
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum KnowledgeDocumentState {
    Active = 1,
    Removed = 2,
}
impl KnowledgeDocumentState {
    fn decode(value: u8) -> Result<Self, KnowledgeIndexError> {
        match value {
            1 => Ok(Self::Active),
            2 => Ok(Self::Removed),
            _ => Err(KnowledgeIndexError::InvalidField("document_state")),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeReferenceTargetKind {
    Task = 1,
    Session = 2,
    Memory = 3,
    Knowledge = 4,
}
impl KnowledgeReferenceTargetKind {
    fn decode(value: u8) -> Result<Self, KnowledgeIndexError> {
        match value {
            1 => Ok(Self::Task),
            2 => Ok(Self::Session),
            3 => Ok(Self::Memory),
            4 => Ok(Self::Knowledge),
            _ => Err(KnowledgeIndexError::InvalidField("reference_target_kind")),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeReferenceScope<'a> {
    Project(ProjectId),
    User(&'a str),
}
impl KnowledgeReferenceScope<'_> {
    fn code(self) -> u8 {
        match self {
            Self::Project(_) => 1,
            Self::User(_) => 2,
        }
    }
}
fn scope_text(scope: KnowledgeReferenceScope<'_>) -> Cow<'_, str> {
    match scope {
        KnowledgeReferenceScope::Project(p) => Cow::Owned(p.to_string()),
        KnowledgeReferenceScope::User(s) => Cow::Borrowed(s),
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnowledgeReferenceNamespace<'a> {
    pub scope: KnowledgeReferenceScope<'a>,
    pub context: Option<&'a str>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnowledgeAuthoredReference<'a> {
    pub target_kind: KnowledgeReferenceTargetKind,
    pub scope: KnowledgeReferenceScope<'a>,
    pub id: &'a str,
    pub context: Option<&'a str>,
    pub relation: &'a str,
    pub field: Option<&'a str>,
}
#[derive(Clone, Copy, Debug)]
pub struct KnowledgeReferenceFields<'a> {
    pub slug: &'a str,
    pub state: KnowledgeDocumentState,
    pub namespace: KnowledgeReferenceNamespace<'a>,
    pub authored: &'a [KnowledgeAuthoredReference<'a>],
}
#[derive(Clone, Copy)]
pub(crate) struct PreparedReferenceRow<'a> {
    pub header: DocumentHeader<'a>,
    pub fields: KnowledgeReferenceFields<'a>,
    pub witness: [u8; 32],
}
pub struct PreparedKnowledgeReferences<'data, 'request> {
    original: &'request SnapshotContext,
    pub(crate) rows: Vec<PreparedReferenceRow<'data>>,
}
impl PreparedKnowledgeReferences<'_, '_> {
    pub(crate) fn check(&self, original: &SnapshotContext) -> Result<(), KnowledgeIndexError> {
        self.original.check()?;
        original.check()?;
        if !self.original.same_request(original) {
            return Err(KnowledgeIndexError::DifferentRequest);
        }
        Ok(())
    }
}
/// Prepare complete witnesses using external text and target-list owners.
/// The result is independent of the temporary fields slice, not its text owners.
/// ```compile_fail
/// use graphit_core::file_io::SnapshotContext;
/// use graphit_data::{knowledge_references::*, knowledge_index::KnowledgeIndexContentLimits,
///     knowledge_document_projection::PreparedKnowledgeDocumentRows};
/// use std::num::NonZeroUsize;
/// fn escape<'a>(original: &'a SnapshotContext, documents: &PreparedKnowledgeDocumentRows<'a, '_>)
///     -> PreparedKnowledgeReferences<'a, 'a> {
///     let refs = vec![KnowledgeAuthoredReference { target_kind: KnowledgeReferenceTargetKind::Task,
///         scope: KnowledgeReferenceScope::User("owner"), id: "task", context: None,
///         relation: "supports", field: None }];
///     let fields = [KnowledgeReferenceFields { slug: "page", state: KnowledgeDocumentState::Active,
///         namespace: KnowledgeReferenceNamespace {scope: KnowledgeReferenceScope::User("owner"),context:None},
///         authored: &refs }];
///     let n = NonZeroUsize::new(10000).unwrap();
///     prepare_knowledge_references(original, documents, &fields, KnowledgeIndexContentLimits {
///         max_identifier_bytes:n }).unwrap()
/// }
/// ```
pub fn prepare_knowledge_references<'data, 'request>(
    original: &'request SnapshotContext,
    documents: &PreparedKnowledgeDocumentRows<'data, '_>,
    fields: &[KnowledgeReferenceFields<'data>],
    limits: KnowledgeIndexContentLimits,
) -> Result<PreparedKnowledgeReferences<'data, 'request>, KnowledgeIndexError> {
    original.check()?;
    let rows = documents.rows(original)?;
    crate::knowledge_index::preflight_document_rows(documents, original, limits, || {
        original.check()?;
        Ok(())
    })?;
    let mut headers = BTreeMap::new();
    for row in rows {
        original.check()?;
        match row {
            MemberRow::DocumentHeader(h) => {
                if headers.insert(h.slug, **h).is_some() {
                    return Err(KnowledgeIndexError::DuplicateSlug);
                }
            }
            MemberRow::CompilationObservation(_) => {
                return Err(KnowledgeIndexError::InvalidField("compilation_observation"));
            }
            _ => {}
        }
    }
    if fields.len() != headers.len() {
        return Err(KnowledgeIndexError::InvalidField("reference_document_map"));
    }
    let mut seen = BTreeSet::new();
    let mut prepared = Vec::new();
    prepared
        .try_reserve_exact(fields.len())
        .map_err(|e| KnowledgeIndexError::Request(io::Error::new(io::ErrorKind::OutOfMemory, e)))?;
    for fields in fields {
        original.check()?;
        let header = *headers
            .get(fields.slug)
            .ok_or(KnowledgeIndexError::InvalidField("reference_document_map"))?;
        if !seen.insert(fields.slug) {
            return Err(KnowledgeIndexError::DuplicateSlug);
        }
        validate_fields(
            *fields,
            header.resolved_wiki_targets.as_slice().len(),
            Some(limits.max_identifier_bytes),
            || {
                original.check()?;
                Ok(())
            },
        )?;
        let witness = witness(
            || {
                original.check()?;
                Ok(())
            },
            WitnessHeader {
                slug: header.slug,
                title: header.title,
                source_path: header.source_path,
                digest: header.compilation_digest.as_bytes(),
            },
            fields.state,
            fields.namespace,
            fields.authored,
            header.resolved_wiki_targets.as_slice(),
        )?;
        prepared.push(PreparedReferenceRow {
            header,
            fields: *fields,
            witness,
        });
    }
    original.check()?;
    Ok(PreparedKnowledgeReferences {
        original,
        rows: prepared,
    })
}
fn identifier(value: &str, max_bytes: Option<NonZeroUsize>) -> Result<(), KnowledgeIndexError> {
    if max_bytes.is_some_and(|max| value.len() > max.get()) {
        return Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"));
    }
    Ok(())
}
fn validate_scope(
    scope: KnowledgeReferenceScope<'_>,
    max_identifier_bytes: Option<NonZeroUsize>,
) -> Result<(), KnowledgeIndexError> {
    let id = scope_text(scope);
    if id.is_empty() {
        return Err(KnowledgeIndexError::InvalidField("reference_scope_id"));
    }
    identifier(&id, max_identifier_bytes)
}
fn validate_fields(
    fields: KnowledgeReferenceFields<'_>,
    wiki_count: usize,
    max_identifier_bytes: Option<NonZeroUsize>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(), KnowledgeIndexError> {
    before()?;
    validate_scope(fields.namespace.scope, max_identifier_bytes)?;
    if let Some(context) = fields.namespace.context {
        if context.is_empty() {
            return Err(KnowledgeIndexError::InvalidField("source_context"));
        }
        identifier(context, max_identifier_bytes)?;
    }
    if fields.state == KnowledgeDocumentState::Removed
        && (!fields.authored.is_empty() || wiki_count != 0)
    {
        return Err(KnowledgeIndexError::InvalidField("removed_references"));
    }
    for reference in fields.authored {
        before()?;
        if reference.id.is_empty() || reference.relation.is_empty() {
            return Err(KnowledgeIndexError::InvalidField("reference_tuple"));
        }
        if reference.context.is_some_and(str::is_empty)
            || (reference.context.is_some()
                && reference.target_kind != KnowledgeReferenceTargetKind::Knowledge)
        {
            return Err(KnowledgeIndexError::InvalidField("reference_context"));
        }
        validate_scope(reference.scope, max_identifier_bytes)?;
        identifier(reference.id, max_identifier_bytes)?;
        if let Some(context) = reference.context {
            identifier(context, max_identifier_bytes)?;
        }
    }
    before()?;
    Ok(())
}
// sha2 0.11 has no std::io::Write implementation. This adapter only hashes
// immutable serialization input; request/admission checks surround that work.
struct HashWriter(Sha256);
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct AuthoredTuple<'a>(KnowledgeAuthoredReference<'a>);
impl Serialize for AuthoredTuple<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let r = self.0;
        let mut t = s.serialize_tuple(7)?;
        t.serialize_element(&(r.target_kind as u8))?;
        t.serialize_element(&r.scope.code())?;
        t.serialize_element(&scope_text(r.scope))?;
        t.serialize_element(r.id)?;
        t.serialize_element(&r.context)?;
        t.serialize_element(r.relation)?;
        t.serialize_element(&r.field)?;
        t.end()
    }
}
struct AuthoredTuples<'a>(&'a [KnowledgeAuthoredReference<'a>]);
impl Serialize for AuthoredTuples<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for r in self.0 {
            seq.serialize_element(&AuthoredTuple(*r))?;
        }
        seq.end()
    }
}
#[derive(Clone, Copy)]
pub(crate) struct WitnessHeader<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub source_path: &'a str,
    pub digest: &'a [u8; 32],
}
fn witness(
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
    header: WitnessHeader<'_>,
    state: KnowledgeDocumentState,
    namespace: KnowledgeReferenceNamespace<'_>,
    authored: &[KnowledgeAuthoredReference<'_>],
    wiki: &[&str],
) -> Result<[u8; 32], KnowledgeIndexError> {
    before()?;
    let mut writer = HashWriter(Sha256::new());
    writer.0.update(b"graphit.knowledge.reference-projection\0");
    serde_json::to_writer(
        &mut writer,
        &(
            1u8,
            header.slug,
            header.title,
            header.source_path,
            header.digest,
            state as u8,
            namespace.scope.code(),
            scope_text(namespace.scope),
            namespace.context,
            AuthoredTuples(authored),
            wiki,
        ),
    )
    .map_err(KnowledgeIndexError::ReferenceSerialization)?;
    let hash = writer.0.finalize().into();
    before()?;
    Ok(hash)
}
pub(crate) fn authored_fields() -> Fields {
    vec![
        Field::new("target_kind", DataType::UInt8, false),
        Field::new("scope_kind", DataType::UInt8, false),
        Field::new("scope_id", DataType::Utf8, false),
        Field::new("id", DataType::Utf8, false),
        Field::new("context", DataType::Utf8, true),
        Field::new("relation", DataType::Utf8, false),
        Field::new("field", DataType::Utf8, true),
    ]
    .into()
}
pub(crate) fn projection_fields() -> Fields {
    vec![
        Field::new("source_scope_kind", DataType::UInt8, false),
        Field::new("source_scope_id", DataType::Utf8, false),
        Field::new("source_context", DataType::Utf8, true),
        Field::new(
            "authored_refs",
            DataType::List(Arc::new(Field::new(
                "item",
                DataType::Struct(authored_fields()),
                false,
            ))),
            false,
        ),
        Field::new("witness", DataType::FixedSizeBinary(32), false),
    ]
    .into()
}
fn authored_builder() -> StructBuilder {
    StructBuilder::new(
        authored_fields(),
        vec![
            Box::new(UInt8Builder::new()),
            Box::new(UInt8Builder::new()),
            Box::new(StringBuilder::new()),
            Box::new(StringBuilder::new()),
            Box::new(StringBuilder::new()),
            Box::new(StringBuilder::new()),
            Box::new(StringBuilder::new()),
        ],
    )
}
fn projection_builder() -> StructBuilder {
    StructBuilder::new(
        projection_fields(),
        vec![
            Box::new(UInt8Builder::new()),
            Box::new(StringBuilder::new()),
            Box::new(StringBuilder::new()),
            Box::new(ListBuilder::new(authored_builder()).with_field(Field::new(
                "item",
                DataType::Struct(authored_fields()),
                false,
            ))),
            Box::new(FixedSizeBinaryBuilder::new(32)),
        ],
    )
}
fn push_ref(b: &mut StructBuilder, r: KnowledgeAuthoredReference<'_>) {
    b.field_builder::<UInt8Builder>(0)
        .unwrap()
        .append_value(r.target_kind as u8);
    b.field_builder::<UInt8Builder>(1)
        .unwrap()
        .append_value(r.scope.code());
    b.field_builder::<StringBuilder>(2)
        .unwrap()
        .append_value(scope_text(r.scope));
    b.field_builder::<StringBuilder>(3)
        .unwrap()
        .append_value(r.id);
    b.field_builder::<StringBuilder>(4)
        .unwrap()
        .append_option(r.context);
    b.field_builder::<StringBuilder>(5)
        .unwrap()
        .append_value(r.relation);
    b.field_builder::<StringBuilder>(6)
        .unwrap()
        .append_option(r.field);
    b.append(true);
}
fn same_header(a: &DocumentHeader<'_>, b: &DocumentHeader<'_>) -> bool {
    let ap = a.recipe.parameters();
    let bp = b.recipe.parameters();
    a.slug == b.slug
        && a.title == b.title
        && a.body == b.body
        && a.kind == b.kind
        && a.summary == b.summary
        && a.description == b.description
        && a.summary_truncated == b.summary_truncated
        && a.source_path == b.source_path
        && a.physical_digest == b.physical_digest
        && a.compilation_digest == b.compilation_digest
        && a.recipe.id() == b.recipe.id()
        && ap.fallback_title == bp.fallback_title
        && ap.kind_on_absent == bp.kind_on_absent
        && ap.max_summary_bytes == bp.max_summary_bytes
        && a.updated_at == b.updated_at
        && a.stale_since == b.stale_since
        && a.stale_after == b.stale_after
        && a.resolved_wiki_targets.as_slice() == b.resolved_wiki_targets.as_slice()
}
pub(crate) fn encode_columns<'a>(
    original: &SnapshotContext,
    headers: impl Iterator<Item = &'a DocumentHeader<'a>>,
    refs: Option<&PreparedKnowledgeReferences<'_, '_>>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<[ArrayRef; 2], KnowledgeIndexError> {
    let mut states = UInt8Builder::new();
    let mut projections = projection_builder();
    let refs = if let Some(r) = refs {
        r.check(original)?;
        Some(&r.rows)
    } else {
        None
    };
    let mut count = 0;
    for h in headers {
        original.check()?;
        before()?;
        let row = refs
            .map(|rs| {
                rs.iter()
                    .find(|r| r.header.slug == h.slug)
                    .ok_or(KnowledgeIndexError::InvalidField("reference_document_map"))
            })
            .transpose()?;
        if let Some(row) = row
            && !same_header(&row.header, h)
        {
            return Err(KnowledgeIndexError::InvalidField("reference_document_map"));
        }
        let state = row.map_or(KnowledgeDocumentState::Active, |r| r.fields.state);
        states.append_value(state as u8);
        append_projection(&mut projections, row, || {
            original.check()?;
            before()
        })?;
        count += 1;
    }
    if refs.is_some_and(|r| r.len() != count) {
        return Err(KnowledgeIndexError::InvalidField("reference_document_map"));
    }
    before()?;
    Ok([Arc::new(states.finish()), Arc::new(projections.finish())])
}

fn append_projection(
    projections: &mut StructBuilder,
    row: Option<&PreparedReferenceRow<'_>>,
    before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(), KnowledgeIndexError> {
    append_projection_fields(projections, row.map(|r| (r.fields, &r.witness)), before)
}

/// A removal has complete empty references and a witness for its retained header.
pub(crate) fn encode_removed_projection(
    header: WitnessHeader<'_>,
    project: ProjectId,
    limits: KnowledgeIndexContentLimits,
    before: impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<ArrayRef, KnowledgeIndexError> {
    before()?;
    let fields = KnowledgeReferenceFields {
        slug: header.slug,
        state: KnowledgeDocumentState::Removed,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project),
            context: None,
        },
        authored: &[],
    };
    validate_fields(fields, 0, Some(limits.max_identifier_bytes), &before)?;
    let computed = witness(&before, header, fields.state, fields.namespace, &[], &[])?;
    let mut builder = projection_builder();
    append_projection_fields(&mut builder, Some((fields, &computed)), &before)?;
    before()?;
    Ok(Arc::new(builder.finish()))
}

fn append_projection_fields(
    projections: &mut StructBuilder,
    row: Option<(KnowledgeReferenceFields<'_>, &[u8; 32])>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(), KnowledgeIndexError> {
    before()?;
    projections
        .field_builder::<UInt8Builder>(0)
        .unwrap()
        .append_value(row.map_or(0, |r| r.0.namespace.scope.code()));
    projections
        .field_builder::<StringBuilder>(1)
        .unwrap()
        .append_value(
            row.map(|r| scope_text(r.0.namespace.scope))
                .unwrap_or(Cow::Borrowed("")),
        );
    projections
        .field_builder::<StringBuilder>(2)
        .unwrap()
        .append_option(row.and_then(|r| r.0.namespace.context));
    let list = projections
        .field_builder::<ListBuilder<StructBuilder>>(3)
        .unwrap();
    if let Some(row) = row {
        for r in row.0.authored {
            before()?;
            push_ref(list.values(), *r);
        }
    }
    list.append(true);
    projections
        .field_builder::<FixedSizeBinaryBuilder>(4)
        .unwrap()
        .append_value(row.map_or(&[0; 32], |r| r.1))?;
    projections.append(row.is_some());
    before()?;
    Ok(())
}

/// Encode first-publication decisions after a complete cumulative preflight.
/// None means incomplete; Some([]) is explicitly known empty.
pub(crate) fn encode_initial_projections<'a>(
    headers: impl Iterator<
        Item = (
            DocumentHeader<'a>,
            Option<&'a [KnowledgeAuthoredReference<'a>]>,
        ),
    > + Clone,
    project: ProjectId,
    limits: KnowledgeIndexContentLimits,
    before: impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<ArrayRef, KnowledgeIndexError> {
    let fields = |header: DocumentHeader<'a>, authored| KnowledgeReferenceFields {
        slug: header.slug,
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project),
            context: None,
        },
        authored,
    };
    for (header, authored) in headers.clone() {
        before()?;
        if let Some(authored) = authored {
            validate_fields(
                fields(header, authored),
                header.resolved_wiki_targets.as_slice().len(),
                Some(limits.max_identifier_bytes),
                &before,
            )?;
        }
    }
    before()?;
    let mut builder = projection_builder();
    for (header, authored) in headers {
        before()?;
        let row = if let Some(authored) = authored {
            let fields = fields(header, authored);
            Some(PreparedReferenceRow {
                header,
                fields,
                witness: witness(
                    &before,
                    WitnessHeader {
                        slug: header.slug,
                        title: header.title,
                        source_path: header.source_path,
                        digest: header.compilation_digest.as_bytes(),
                    },
                    fields.state,
                    fields.namespace,
                    fields.authored,
                    header.resolved_wiki_targets.as_slice(),
                )?,
            })
        } else {
            None
        };
        append_projection(&mut builder, row.as_ref(), &before)?;
    }
    before()?;
    Ok(Arc::new(builder.finish()))
}

/// Build a replacement projection using C8's validators and witness. Preserved
/// Arrow text owners remain borrowed inside the visitor; only the witness changes.
pub(crate) fn update_projection(
    header: DocumentHeader<'_>,
    previous: ReferenceRowInput<'_>,
    authored: Option<&[KnowledgeAuthoredReference<'_>]>,
    project: ProjectId,
    limits: KnowledgeIndexContentLimits,
    before: impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<ArrayRef, KnowledgeIndexError> {
    before()?;
    if previous.projection.is_null(previous.row) && authored.is_none() {
        return Ok(Arc::new(previous.projection.slice(previous.row, 1)));
    }
    let new_header = WitnessHeader {
        slug: header.slug,
        title: header.title,
        source_path: header.source_path,
        digest: header.compilation_digest.as_bytes(),
    };
    if let Some(authored) = authored {
        // A native full scan already proved the stored source namespace equals
        // this live project with no context, including a previous Some value.
        let fields = KnowledgeReferenceFields {
            slug: header.slug,
            state: KnowledgeDocumentState::Active,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(project),
                context: None,
            },
            authored,
        };
        validate_fields(
            fields,
            header.resolved_wiki_targets.as_slice().len(),
            Some(limits.max_identifier_bytes),
            &before,
        )?;
        let witness = witness(
            &before,
            new_header,
            fields.state,
            fields.namespace,
            fields.authored,
            header.resolved_wiki_targets.as_slice(),
        )?;
        let row = PreparedReferenceRow {
            header,
            fields,
            witness,
        };
        let mut builder = projection_builder();
        append_projection(&mut builder, Some(&row), &before)?;
        return Ok(Arc::new(builder.finish()));
    }
    preserve_projection(
        new_header,
        header.resolved_wiki_targets.as_slice(),
        previous,
        limits,
        before,
    )
}

/// C8 Preserve over validated ordinary headers; no new document authority.
pub(crate) fn preserve_projection(
    new_header: WitnessHeader<'_>,
    wiki_targets: &[&str],
    previous: ReferenceRowInput<'_>,
    limits: KnowledgeIndexContentLimits,
    before: impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<ArrayRef, KnowledgeIndexError> {
    before()?;
    if previous.projection.is_null(previous.row) {
        return Ok(Arc::new(previous.projection.slice(previous.row, 1)));
    }
    let projection_fields = previous.projection.fields().clone();
    let mut columns = previous
        .projection
        .slice(previous.row, 1)
        .columns()
        .to_vec();
    visit_row(
        previous,
        Some(limits.max_identifier_bytes),
        &before,
        |fields, _| {
            let computed = witness(
                &before,
                new_header,
                KnowledgeDocumentState::Active,
                fields.namespace,
                fields.authored,
                wiki_targets,
            )?;
            columns[4] = Arc::new(FixedSizeBinaryArray::try_from_iter(
                [computed.as_slice()].into_iter(),
            )?);
            Ok(())
        },
    )?;
    before()?;
    Ok(Arc::new(StructArray::try_new(
        projection_fields,
        columns,
        None,
    )?))
}

fn column<T: Array + 'static>(
    columns: &StructArray,
    index: usize,
) -> Result<&T, KnowledgeIndexError> {
    columns
        .column(index)
        .as_any()
        .downcast_ref()
        .ok_or(KnowledgeIndexError::InvalidField("reference_array_type"))
}
fn decode_scope<'a>(
    kind: u8,
    id: &'a str,
) -> Result<KnowledgeReferenceScope<'a>, KnowledgeIndexError> {
    if id.is_empty() {
        return Err(KnowledgeIndexError::InvalidField("reference_scope_id"));
    }
    match kind {
        1 => id
            .parse()
            .map(KnowledgeReferenceScope::Project)
            .map_err(KnowledgeIndexError::Request),
        2 => Ok(KnowledgeReferenceScope::User(id)),
        _ => Err(KnowledgeIndexError::InvalidField("reference_scope_kind")),
    }
}
/// Validate the complete row before exposing its transient borrowed projection.
/// The visitor cannot return references into the temporary Arrow slice owner.
pub(crate) struct ReferenceRowInput<'a> {
    pub projection: &'a StructArray,
    pub state: u8,
    pub row: usize,
    pub header: WitnessHeader<'a>,
    pub wiki: &'a StringArray,
    pub expected_project: Option<ProjectId>,
}
pub(crate) fn visit_row(
    input: ReferenceRowInput<'_>,
    max_identifier_bytes: Option<NonZeroUsize>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
    visit: impl FnOnce(KnowledgeReferenceFields<'_>, [u8; 32]) -> Result<(), KnowledgeIndexError>,
) -> Result<KnowledgeDocumentState, KnowledgeIndexError> {
    let ReferenceRowInput {
        projection,
        state,
        row,
        header,
        wiki,
        expected_project,
    } = input;
    before()?;
    let state = KnowledgeDocumentState::decode(state)?;
    if projection.is_null(row) {
        if state == KnowledgeDocumentState::Removed {
            return Err(KnowledgeIndexError::InvalidField("removed_projection"));
        }
        return Ok(state);
    }
    for (field, array) in projection.fields().iter().zip(projection.columns()) {
        if !field.is_nullable() && array.is_null(row) {
            return Err(KnowledgeIndexError::InvalidField("reference_null_required"));
        }
    }
    let kind: &UInt8Array = column(projection, 0)?;
    let scope_id: &StringArray = column(projection, 1)?;
    let context: &StringArray = column(projection, 2)?;
    let authored: &ListArray = column(projection, 3)?;
    let stored: &FixedSizeBinaryArray = column(projection, 4)?;
    let namespace = KnowledgeReferenceNamespace {
        scope: decode_scope(kind.value(row), scope_id.value(row))?,
        context: (!context.is_null(row)).then(|| context.value(row)),
    };
    if expected_project.is_some_and(|id| {
        namespace.scope != KnowledgeReferenceScope::Project(id) || namespace.context.is_some()
    }) {
        return Err(KnowledgeIndexError::InvalidField("source_namespace"));
    }
    let raw = authored.value(row);
    let raw = raw
        .as_any()
        .downcast_ref::<StructArray>()
        .ok_or(KnowledgeIndexError::InvalidField("reference_array_type"))?;
    if raw.null_count() != 0 {
        return Err(KnowledgeIndexError::InvalidField("reference_null_entry"));
    }
    let kinds: &UInt8Array = column(raw, 0)?;
    let scopes: &UInt8Array = column(raw, 1)?;
    let scope_ids: &StringArray = column(raw, 2)?;
    let ids: &StringArray = column(raw, 3)?;
    let contexts: &StringArray = column(raw, 4)?;
    let relations: &StringArray = column(raw, 5)?;
    let fields: &StringArray = column(raw, 6)?;
    for (field, array) in raw.fields().iter().zip(raw.columns()) {
        if !field.is_nullable() && array.null_count() != 0 {
            return Err(KnowledgeIndexError::InvalidField("reference_null_required"));
        }
    }
    let mut refs = Vec::new();
    refs.try_reserve_exact(raw.len())
        .map_err(|e| KnowledgeIndexError::Request(io::Error::new(io::ErrorKind::OutOfMemory, e)))?;
    for index in 0..raw.len() {
        before()?;
        refs.push(KnowledgeAuthoredReference {
            target_kind: KnowledgeReferenceTargetKind::decode(kinds.value(index))?,
            scope: decode_scope(scopes.value(index), scope_ids.value(index))?,
            id: ids.value(index),
            context: (!contexts.is_null(index)).then(|| contexts.value(index)),
            relation: relations.value(index),
            field: (!fields.is_null(index)).then(|| fields.value(index)),
        });
    }
    let f = KnowledgeReferenceFields {
        slug: header.slug,
        state,
        namespace,
        authored: &refs,
    };
    validate_fields(f, wiki.len(), max_identifier_bytes, &mut before)?;
    let mut targets = Vec::new();
    targets
        .try_reserve_exact(wiki.len())
        .map_err(|e| KnowledgeIndexError::Request(io::Error::new(io::ErrorKind::OutOfMemory, e)))?;
    for value in wiki.iter() {
        before()?;
        targets.push(value.ok_or(KnowledgeIndexError::InvalidField("target_null"))?);
    }
    let computed = witness(&mut before, header, state, namespace, &refs, &targets)?;
    if stored.value(row) != computed {
        return Err(KnowledgeIndexError::InvalidField("reference_witness"));
    }
    before()?;
    visit(f, computed)?;
    before()?;
    Ok(state)
}
pub(crate) fn validate_prepared(
    refs: &PreparedKnowledgeReferences<'_, '_>,
    limits: KnowledgeIndexContentLimits,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(), KnowledgeIndexError> {
    for row in &refs.rows {
        before()?;
        validate_fields(
            row.fields,
            row.header.resolved_wiki_targets.as_slice().len(),
            Some(limits.max_identifier_bytes),
            &mut before,
        )?;
    }
    before()
}
pub(crate) fn check_native_source(
    refs: &PreparedKnowledgeReferences<'_, '_>,
    project: ProjectId,
) -> Result<(), KnowledgeIndexError> {
    for row in &refs.rows {
        if row.fields.namespace.scope != KnowledgeReferenceScope::Project(project)
            || row.fields.namespace.context.is_some()
        {
            return Err(KnowledgeIndexError::InvalidField("source_namespace"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::{Duration, Instant};

    fn header() -> WitnessHeader<'static> {
        WitnessHeader {
            slug: "page",
            title: "title\n\"é\\",
            source_path: "docs/page.md",
            digest: &[7; 32],
        }
    }

    fn namespace() -> KnowledgeReferenceNamespace<'static> {
        KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::User("owner"),
            context: Some(""),
        }
    }

    #[test]
    fn witness_hashes_canonical_json_with_boundary_guards() {
        for repeats in [1, 4096] {
            let relation = "supports\n\"é\\".repeat(repeats);
            let refs = [KnowledgeAuthoredReference {
                target_kind: KnowledgeReferenceTargetKind::Task,
                scope: KnowledgeReferenceScope::User("target"),
                id: "tsk-test",
                context: None,
                relation: &relation,
                field: Some(""),
            }; 2];
            let mut guards = 0;
            let actual = witness(
                || {
                    guards += 1;
                    Ok(())
                },
                header(),
                KnowledgeDocumentState::Active,
                namespace(),
                &refs,
                &["wiki", "wiki", ""],
            )
            .unwrap();
            // Independent tuple construction protects the persisted JSON format,
            // including escapes, nullable fields, order and duplicate entries.
            let reference = serde_json::json!([1, 2, "target", "tsk-test", null, relation, ""]);
            let digest = [7u8; 32];
            let json = serde_json::json!([
                1,
                "page",
                "title\n\"é\\",
                "docs/page.md",
                digest,
                1,
                2,
                "owner",
                "",
                [reference.clone(), reference],
                ["wiki", "wiki", ""]
            ]);
            let mut expected = Sha256::new();
            expected.update(b"graphit.knowledge.reference-projection\0");
            expected.update(serde_json::to_vec(&json).unwrap());
            assert_eq!(actual, <[u8; 32]>::from(expected.finalize()));
            assert_eq!(guards, 2, "authorization is independent of JSON fragments");
        }
    }

    #[test]
    fn witness_boundary_failure_preserves_original_io_cause() {
        for boundary in [1, 2] {
            let cause = Box::new(io::Error::other("original guard cause"));
            // Box allocation owns the exact inner IO object through the guard.
            let pointer = &*cause as *const io::Error;
            let cause: Box<dyn std::error::Error + Send + Sync> = cause;
            let mut error = Some(io::Error::new(io::ErrorKind::Interrupted, cause));
            let mut guards = 0;
            let result = witness(
                || {
                    guards += 1;
                    if guards == boundary {
                        Err(KnowledgeIndexError::Request(error.take().unwrap()))
                    } else {
                        Ok(())
                    }
                },
                header(),
                KnowledgeDocumentState::Active,
                namespace(),
                &[],
                &[],
            );
            let KnowledgeIndexError::Request(actual) = result.unwrap_err() else {
                panic!("guard error category changed");
            };
            assert_eq!(actual.kind(), io::ErrorKind::Interrupted);
            assert!(std::ptr::eq(
                actual
                    .get_ref()
                    .unwrap()
                    .downcast_ref::<io::Error>()
                    .unwrap(),
                pointer,
            ));
            assert_eq!(guards, boundary, "a failed guard is never retried");
        }
    }

    #[test]
    fn witness_boundaries_preserve_request_cancellation_and_deadline() {
        let cancellation = Cancellation::default();
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(60),
            cancellation.clone(),
        );
        let mut guards = 0;
        let result = witness(
            || {
                guards += 1;
                original.check()?;
                if guards == 1 {
                    cancellation.cancel();
                }
                Ok(())
            },
            header(),
            KnowledgeDocumentState::Active,
            namespace(),
            &[],
            &[],
        );
        assert!(
            matches!(result, Err(KnowledgeIndexError::Request(e)) if e.kind() == io::ErrorKind::Interrupted)
        );
        assert_eq!(guards, 2);

        let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
        let result = witness(
            || {
                expired.check()?;
                Ok(())
            },
            header(),
            KnowledgeDocumentState::Active,
            namespace(),
            &[],
            &[],
        );
        assert!(
            matches!(result, Err(KnowledgeIndexError::Request(e)) if e.kind() == io::ErrorKind::TimedOut)
        );
    }
}
