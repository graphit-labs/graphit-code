//! Persisted document state and authored references.
//! These values never grant access or select another SDK generation.
use crate::knowledge_document_projection::{
    DocumentHeader, MemberRow, PreparedKnowledgeDocumentRows,
};
use crate::knowledge_index::{KnowledgeIndexContentLimits, KnowledgeIndexError};
use arrow_array::{
    Array, ArrayRef, ListArray, StringArray, StructArray, UInt8Array,
    builder::{ListBuilder, StringBuilder, StructBuilder, UInt8Builder},
};
use arrow_schema::{DataType, Field, Fields};
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use serde::Serialize;
use std::{borrow::Cow, collections::BTreeMap, io, num::NonZeroUsize, sync::Arc};

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
/// Reference fields associated with a document by its slug.
pub struct PreparedKnowledgeReferences<'a> {
    rows: BTreeMap<&'a str, KnowledgeReferenceFields<'a>>,
}

pub fn prepare_knowledge_references<'data>(
    original: &SnapshotContext,
    documents: &PreparedKnowledgeDocumentRows<'data, '_>,
    fields: &[KnowledgeReferenceFields<'data>],
    limits: KnowledgeIndexContentLimits,
) -> Result<PreparedKnowledgeReferences<'data>, KnowledgeIndexError> {
    let mut wiki_counts = BTreeMap::new();
    for row in documents.rows(original)? {
        original.check()?;
        if let MemberRow::DocumentHeader(header) = row
            && wiki_counts
                .insert(header.slug, header.resolved_wiki_targets.as_slice().len())
                .is_some()
        {
            return Err(KnowledgeIndexError::DuplicateSlug);
        }
    }
    if fields.len() != wiki_counts.len() {
        return Err(KnowledgeIndexError::InvalidField("reference_document_map"));
    }
    let mut rows = BTreeMap::new();
    for fields in fields {
        original.check()?;
        let count = wiki_counts
            .get(fields.slug)
            .ok_or(KnowledgeIndexError::InvalidField("reference_document_map"))?;
        validate_fields(*fields, *count, Some(limits.max_identifier_bytes), || {
            original.check()?;
            Ok(())
        })?;
        if rows.insert(fields.slug, *fields).is_some() {
            return Err(KnowledgeIndexError::DuplicateSlug);
        }
    }
    Ok(PreparedKnowledgeReferences { rows })
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
    ]
    .into()
}
pub(crate) fn legacy_projection_fields() -> Fields {
    let mut fields: Vec<_> = projection_fields().iter().cloned().collect();
    fields.push(Arc::new(Field::new(
        "witness",
        DataType::FixedSizeBinary(32),
        false,
    )));
    fields.into()
}

pub(crate) fn without_legacy_witness(
    projection: &StructArray,
) -> Result<StructArray, KnowledgeIndexError> {
    if projection.fields() == &projection_fields() {
        return Ok(projection.clone());
    }
    if projection.fields() != &legacy_projection_fields() {
        return Err(KnowledgeIndexError::InvalidField("reference_projection"));
    }
    Ok(StructArray::try_new(
        projection_fields(),
        projection.columns()[..4].to_vec(),
        projection.nulls().cloned(),
    )?)
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
pub(crate) fn encode_columns<'a>(
    headers: impl Iterator<Item = &'a DocumentHeader<'a>>,
    refs: Option<&PreparedKnowledgeReferences<'_>>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<[ArrayRef; 2], KnowledgeIndexError> {
    let mut states = UInt8Builder::new();
    let mut projections = projection_builder();
    let mut count = 0;
    for header in headers {
        before()?;
        if let Some(refs) = refs {
            let fields = *refs
                .rows
                .get(header.slug)
                .ok_or(KnowledgeIndexError::InvalidField("reference_document_map"))?;
            let targets = header.resolved_wiki_targets.as_slice();
            if fields.state == KnowledgeDocumentState::Removed && !targets.is_empty() {
                return Err(KnowledgeIndexError::InvalidField("removed_references"));
            }
            states.append_value(fields.state as u8);
            append_projection_fields(&mut projections, Some(fields), &mut before)?;
        } else {
            states.append_value(KnowledgeDocumentState::Active as u8);
            append_projection_fields(&mut projections, None, &mut before)?;
        }
        count += 1;
    }
    if refs.is_some_and(|refs| refs.rows.len() != count) {
        return Err(KnowledgeIndexError::InvalidField("reference_document_map"));
    }
    before()?;
    Ok([Arc::new(states.finish()), Arc::new(projections.finish())])
}

/// A removal has complete empty references.
pub(crate) fn encode_removed_projection(
    slug: &str,
    project: ProjectId,
    limits: KnowledgeIndexContentLimits,
    before: impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<ArrayRef, KnowledgeIndexError> {
    before()?;
    let fields = KnowledgeReferenceFields {
        slug,
        state: KnowledgeDocumentState::Removed,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project),
            context: None,
        },
        authored: &[],
    };
    validate_fields(fields, 0, Some(limits.max_identifier_bytes), &before)?;
    let mut builder = projection_builder();
    append_projection_fields(&mut builder, Some(fields), &before)?;
    before()?;
    Ok(Arc::new(builder.finish()))
}

fn append_projection_fields(
    projections: &mut StructBuilder,
    row: Option<KnowledgeReferenceFields<'_>>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(), KnowledgeIndexError> {
    before()?;
    projections
        .field_builder::<UInt8Builder>(0)
        .unwrap()
        .append_value(row.map_or(0, |r| r.namespace.scope.code()));
    projections
        .field_builder::<StringBuilder>(1)
        .unwrap()
        .append_value(
            row.map(|r| scope_text(r.namespace.scope))
                .unwrap_or(Cow::Borrowed("")),
        );
    projections
        .field_builder::<StringBuilder>(2)
        .unwrap()
        .append_option(row.and_then(|r| r.namespace.context));
    let list = projections
        .field_builder::<ListBuilder<StructBuilder>>(3)
        .unwrap();
    if let Some(row) = row {
        for r in row.authored {
            before()?;
            push_ref(list.values(), *r);
        }
    }
    list.append(true);
    projections.append(row.is_some());
    before()?;
    Ok(())
}

/// None means incomplete; Some([]) is explicitly known empty.
pub(crate) fn encode_initial_projections<'a>(
    headers: impl Iterator<
        Item = (
            DocumentHeader<'a>,
            Option<&'a [KnowledgeAuthoredReference<'a>]>,
        ),
    >,
    project: ProjectId,
    limits: KnowledgeIndexContentLimits,
    before: impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<ArrayRef, KnowledgeIndexError> {
    let mut builder = projection_builder();
    for (header, authored) in headers {
        before()?;
        if let Some(authored) = authored {
            let fields = KnowledgeReferenceFields {
                slug: header.slug,
                state: KnowledgeDocumentState::Active,
                namespace: KnowledgeReferenceNamespace {
                    scope: KnowledgeReferenceScope::Project(project),
                    context: None,
                },
                authored,
            };
            let targets = header.resolved_wiki_targets.as_slice();
            validate_fields(
                fields,
                targets.len(),
                Some(limits.max_identifier_bytes),
                &before,
            )?;
            append_projection_fields(&mut builder, Some(fields), &before)?;
        } else {
            append_projection_fields(&mut builder, None, &before)?;
        }
    }
    Ok(Arc::new(builder.finish()))
}

/// Omitted authored references preserve the already validated projection.
pub(crate) fn update_projection(
    header: DocumentHeader<'_>,
    previous: &StructArray,
    row: usize,
    authored: Option<&[KnowledgeAuthoredReference<'_>]>,
    project: ProjectId,
    limits: KnowledgeIndexContentLimits,
    before: impl Fn() -> Result<(), KnowledgeIndexError>,
) -> Result<ArrayRef, KnowledgeIndexError> {
    let Some(authored) = authored else {
        return Ok(Arc::new(without_legacy_witness(&previous.slice(row, 1))?));
    };
    let fields = KnowledgeReferenceFields {
        slug: header.slug,
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project),
            context: None,
        },
        authored,
    };
    validate_fields(fields, 0, Some(limits.max_identifier_bytes), &before)?;
    let mut builder = projection_builder();
    append_projection_fields(&mut builder, Some(fields), before)?;
    Ok(Arc::new(builder.finish()))
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
    pub slug: &'a str,
    pub wiki_count: usize,
    pub expected_project: Option<ProjectId>,
}
pub(crate) fn visit_row(
    input: ReferenceRowInput<'_>,
    max_identifier_bytes: Option<NonZeroUsize>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
    visit: impl FnOnce(KnowledgeReferenceFields<'_>) -> Result<(), KnowledgeIndexError>,
) -> Result<KnowledgeDocumentState, KnowledgeIndexError> {
    let ReferenceRowInput {
        projection,
        state,
        row,
        slug,
        wiki_count,
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
    for (field, array) in projection.fields().iter().zip(projection.columns()).take(4) {
        if !field.is_nullable() && array.is_null(row) {
            return Err(KnowledgeIndexError::InvalidField("reference_null_required"));
        }
    }
    let kind: &UInt8Array = column(projection, 0)?;
    let scope_id: &StringArray = column(projection, 1)?;
    let context: &StringArray = column(projection, 2)?;
    let authored: &ListArray = column(projection, 3)?;
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
        slug,
        state,
        namespace,
        authored: &refs,
    };
    validate_fields(f, wiki_count, max_identifier_bytes, &mut before)?;
    visit(f)?;
    before()?;
    Ok(state)
}
pub(crate) fn check_native_source(
    refs: &PreparedKnowledgeReferences<'_>,
    project: ProjectId,
) -> Result<(), KnowledgeIndexError> {
    for fields in refs.rows.values() {
        if fields.namespace.scope != KnowledgeReferenceScope::Project(project)
            || fields.namespace.context.is_some()
        {
            return Err(KnowledgeIndexError::InvalidField("source_namespace"));
        }
    }
    Ok(())
}
