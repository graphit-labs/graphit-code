//! Native Knowledge provenance queries over one actual SDK pin.
//! Metadata observations and closed continuations never grant access.
use crate::{
    knowledge_document_projection::CompilationDigest32,
    knowledge_index::{
        self as index, KnowledgeGeneration, KnowledgeIndexError, KnowledgeIndexReadLimits,
    },
    knowledge_references::{
        self as refs, KnowledgeDocumentState, KnowledgeReferenceNamespace, KnowledgeReferenceScope,
        KnowledgeReferenceTargetKind,
    },
    lance_module_store::{ModuleSnapshot, StoreError},
};
use arrow_array::{
    Array, FixedSizeBinaryArray, ListArray, RecordBatch, StringArray, StructArray,
    TimestampNanosecondArray, UInt8Array,
};
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::{
    artifact_store::StoreOwner, file_io::SnapshotContext, project_identity::ProjectId,
};
use lance::dataset::scanner::Scanner;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    num::NonZeroUsize,
    sync::Arc,
};

#[derive(Clone, Copy, Debug)]
pub struct KnowledgeReferenceTarget<'a> {
    pub target_kind: KnowledgeReferenceTargetKind,
    pub namespace: KnowledgeReferenceNamespace<'a>,
    pub id: &'a str,
}
#[derive(Clone, Copy)]
pub struct KnowledgeReferenceQuery<'a> {
    pub outgoing: Option<&'a str>,
    pub incoming: Option<KnowledgeReferenceTarget<'a>>,
    pub page_size: NonZeroUsize,
    pub outgoing_cursor: Option<&'a KnowledgeReferencesCursor>,
    pub incoming_cursor: Option<&'a KnowledgeReferencesCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgeReferenceOwnedScope {
    Project(ProjectId),
    User(String),
}
impl KnowledgeReferenceOwnedScope {
    fn borrow(&self) -> KnowledgeReferenceScope<'_> {
        match self {
            Self::Project(id) => KnowledgeReferenceScope::Project(*id),
            Self::User(id) => KnowledgeReferenceScope::User(id),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeReferenceOwnedNamespace {
    pub scope: KnowledgeReferenceOwnedScope,
    pub context: Option<String>,
}
impl KnowledgeReferenceOwnedNamespace {
    fn borrow(&self) -> KnowledgeReferenceNamespace<'_> {
        KnowledgeReferenceNamespace {
            scope: self.scope.borrow(),
            context: self.context.as_deref(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnowledgeReferenceOwnedTarget {
    pub target_kind: KnowledgeReferenceTargetKind,
    pub namespace: KnowledgeReferenceOwnedNamespace,
    pub id: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnowledgeReferenceSource {
    pub slug: String,
    pub title: String,
    pub source_path: String,
    pub compilation_digest: CompilationDigest32,
    pub namespace: Option<KnowledgeReferenceOwnedNamespace>,
    pub witness: Option<[u8; 32]>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeReferencePayload {
    Authored {
        target: KnowledgeReferenceOwnedTarget,
        relation: String,
        field: Option<String>,
    },
    WikiLink {
        target: KnowledgeReferenceOwnedTarget,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeReferenceProvenance {
    Authored,
    WikiLink,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeReferenceObservation {
    KnowledgeHead { slug: String, title: String },
    Unresolved,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnowledgeReferenceEdge {
    pub source: KnowledgeReferenceSource,
    pub payload: KnowledgeReferencePayload,
    pub provenance: KnowledgeReferenceProvenance,
    pub ordinal: usize,
    pub target_observation: KnowledgeReferenceObservation,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnowledgeReferencePage {
    pub results: Vec<KnowledgeReferenceEdge>,
    pub total: usize,
    pub complete: bool,
    pub next_cursor: Option<KnowledgeReferencesCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeReferencesWarning {
    MissingCurrentProjection { source: KnowledgeReferenceSource },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnowledgeReferencesCoverage {
    pub knowledge_source_complete: bool,
    pub inbound_other_sources_not_qualified: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnowledgeReferenceSnapshotRead {
    /// Validated outgoing focus from this pin, including a source with no edges.
    /// Absent when outgoing was not requested; this observation grants no access.
    pub focused_source: Option<KnowledgeReferenceSource>,
    pub outgoing: Option<Box<KnowledgeReferencePage>>,
    pub incoming: Option<Box<KnowledgeReferencePage>>,
    pub coverage: KnowledgeReferencesCoverage,
    pub warnings: Vec<KnowledgeReferencesWarning>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeReferencesRead {
    NoCorpus,
    Published {
        generation: KnowledgeGeneration,
        read: Box<KnowledgeReferenceSnapshotRead>,
    },
}
#[derive(Debug)]
pub enum KnowledgeReferenceQueryError {
    Data(KnowledgeIndexError),
    InvalidQuery(&'static str),
    InvalidCursor,
    UnknownSource,
}
impl From<KnowledgeIndexError> for KnowledgeReferenceQueryError {
    fn from(e: KnowledgeIndexError) -> Self {
        Self::Data(e)
    }
}
impl From<StoreError> for KnowledgeReferenceQueryError {
    fn from(e: StoreError) -> Self {
        Self::Data(e.into())
    }
}
impl fmt::Display for KnowledgeReferenceQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(e) => write!(f, "Knowledge references: {e}"),
            Self::InvalidQuery(field) => write!(f, "invalid Knowledge reference query: {field}"),
            Self::InvalidCursor => {
                f.write_str("Knowledge reference continuation no longer matches")
            }
            Self::UnknownSource => f.write_str("Knowledge reference source is absent"),
        }
    }
}
impl Error for KnowledgeReferenceQueryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Data(e) => Some(e),
            _ => None,
        }
    }
}
type Result<T> = std::result::Result<T, KnowledgeReferenceQueryError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Direction {
    Outgoing,
    Incoming,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Selection {
    Native {
        project_id: ProjectId,
        selected_lance_branch: String,
    },
    CallerPin,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetBinding {
    kind: u8,
    scope: KnowledgeReferenceOwnedScope,
    id: String,
    context: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    uri: String,
    version: u64,
    physical_branch: Option<String>,
    inherited_target: Option<String>,
    selection: Selection,
    outgoing: Option<String>,
    incoming: Option<TargetBinding>,
    max_identifier: usize,
}
/// Closed observation data; every continuation requires a fresh live read boundary.
/// Fields cannot be changed by a Rust caller; Serde input is still ordinary data
/// and must pass the live generation, query and identifier-policy bindings on every read.
///
/// ```compile_fail
/// use graphit_data::knowledge_reference_query::KnowledgeReferencesCursor;
/// fn change_offset(mut cursor: KnowledgeReferencesCursor) {
///     cursor.offset = 0;
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeReferencesCursor {
    binding: Box<Binding>,
    direction: Direction,
    offset: usize,
}

fn owned_namespace(ns: KnowledgeReferenceNamespace<'_>) -> KnowledgeReferenceOwnedNamespace {
    KnowledgeReferenceOwnedNamespace {
        scope: match ns.scope {
            KnowledgeReferenceScope::Project(id) => KnowledgeReferenceOwnedScope::Project(id),
            KnowledgeReferenceScope::User(id) => KnowledgeReferenceOwnedScope::User(id.to_owned()),
        },
        context: ns.context.map(str::to_owned),
    }
}
fn identifier_input(value: &str, limits: KnowledgeIndexReadLimits) -> Result<()> {
    if value.is_empty() || value.len() > limits.content.max_identifier_bytes.get() {
        return Err(KnowledgeReferenceQueryError::InvalidQuery("identifier"));
    }
    Ok(())
}
fn target_input(
    target: KnowledgeReferenceTarget<'_>,
    limits: KnowledgeIndexReadLimits,
) -> Result<()> {
    identifier_input(target.id, limits)?;
    if let KnowledgeReferenceScope::User(id) = target.namespace.scope {
        identifier_input(id, limits)?;
    }
    if let Some(context) = target.namespace.context {
        if target.target_kind != KnowledgeReferenceTargetKind::Knowledge {
            return Err(KnowledgeReferenceQueryError::InvalidQuery("target_context"));
        }
        identifier_input(context, limits)?;
    }
    Ok(())
}
fn decode_kind(kind: u8) -> Result<KnowledgeReferenceTargetKind> {
    Ok(match kind {
        1 => KnowledgeReferenceTargetKind::Task,
        2 => KnowledgeReferenceTargetKind::Session,
        3 => KnowledgeReferenceTargetKind::Memory,
        4 => KnowledgeReferenceTargetKind::Knowledge,
        _ => return Err(KnowledgeReferenceQueryError::InvalidCursor),
    })
}
fn input(query: KnowledgeReferenceQuery<'_>, limits: KnowledgeIndexReadLimits) -> Result<()> {
    if query.outgoing.is_none() && query.incoming.is_none() {
        return Err(KnowledgeReferenceQueryError::InvalidQuery("focus"));
    }
    if query.page_size.get() > 100 {
        return Err(KnowledgeReferenceQueryError::InvalidQuery("page_size"));
    }
    if let Some(slug) = query.outgoing {
        identifier_input(slug, limits)?;
        index::validate_slug(slug)?;
    }
    if let Some(target) = query.incoming {
        target_input(target, limits)?;
    }
    for (cursor, focus, direction) in [
        (
            query.outgoing_cursor,
            query.outgoing.is_some(),
            Direction::Outgoing,
        ),
        (
            query.incoming_cursor,
            query.incoming.is_some(),
            Direction::Incoming,
        ),
    ] {
        let Some(cursor) = cursor else { continue };
        if !focus || cursor.direction != direction {
            return Err(KnowledgeReferenceQueryError::InvalidCursor);
        }
        let binding = &cursor.binding;
        if let Some(slug) = binding.outgoing.as_deref() {
            identifier_input(slug, limits)?;
            index::validate_slug(slug)?;
        }
        if let Some(target) = &binding.incoming {
            target_input(
                KnowledgeReferenceTarget {
                    target_kind: decode_kind(target.kind)?,
                    namespace: KnowledgeReferenceNamespace {
                        scope: target.scope.borrow(),
                        context: target.context.as_deref(),
                    },
                    id: &target.id,
                },
                limits,
            )?;
        }
    }
    Ok(())
}
fn binding(
    generation: &KnowledgeGeneration,
    selection: Selection,
    query: KnowledgeReferenceQuery<'_>,
    limits: KnowledgeIndexReadLimits,
) -> Binding {
    Binding {
        uri: generation.corpus_uri.clone(),
        version: generation.version,
        physical_branch: generation.physical_branch.clone(),
        inherited_target: generation.inherited_target.clone(),
        selection,
        outgoing: query.outgoing.map(str::to_owned),
        incoming: query.incoming.map(|t| {
            let ns = owned_namespace(t.namespace);
            TargetBinding {
                kind: t.target_kind as u8,
                scope: ns.scope,
                id: t.id.to_owned(),
                context: ns.context,
            }
        }),
        max_identifier: limits.content.max_identifier_bytes.get(),
    }
}
fn check_cursors(query: KnowledgeReferenceQuery<'_>, binding: &Binding) -> Result<()> {
    for c in [query.outgoing_cursor, query.incoming_cursor]
        .into_iter()
        .flatten()
    {
        if c.binding.as_ref() != binding {
            return Err(KnowledgeReferenceQueryError::InvalidCursor);
        }
    }
    Ok(())
}

/// Read one caller-owned pin with row/EOF authorization supplied by the caller.
pub async fn read_knowledge_reference_snapshot_guarded(
    original: &Arc<SnapshotContext>,
    snapshot: &ModuleSnapshot,
    generation: &KnowledgeGeneration,
    query: KnowledgeReferenceQuery<'_>,
    limits: KnowledgeIndexReadLimits,
    mut before: impl FnMut() -> std::result::Result<(), KnowledgeIndexError>,
) -> Result<KnowledgeReferenceSnapshotRead> {
    original.check().map_err(KnowledgeIndexError::from)?;
    before()?;
    input(query, limits)?;
    if snapshot.corpus_uri() != generation.corpus_uri || snapshot.version() != generation.version {
        return Err(KnowledgeReferenceQueryError::InvalidCursor);
    }
    crate::knowledge_index::semantic::dimensions(snapshot.schema())?;
    read_knowledge_reference_scan_guarded(
        original,
        snapshot.scan(),
        generation,
        query,
        limits,
        None,
        before,
    )
    .await
}
/// The caller supplies an already captured, schema/generation-checked pin.
/// This shared kernel retains namespace validation and the actual row/EOF guard.
pub(crate) async fn read_knowledge_reference_scan_guarded(
    original: &Arc<SnapshotContext>,
    scanner: lance::dataset::scanner::Scanner,
    generation: &KnowledgeGeneration,
    query: KnowledgeReferenceQuery<'_>,
    limits: KnowledgeIndexReadLimits,
    expected_project: Option<ProjectId>,
    mut before: impl FnMut() -> std::result::Result<(), KnowledgeIndexError>,
) -> Result<KnowledgeReferenceSnapshotRead> {
    original.check().map_err(KnowledgeIndexError::from)?;
    before()?;
    input(query, limits)?;
    let b = binding(generation, Selection::CallerPin, query, limits);
    check_cursors(query, &b)?;
    collect(
        original,
        scanner,
        expected_project,
        query,
        limits,
        b,
        before,
    )
    .await
}

/// Resolve the Native path/selector exclusively from a live PROJECT Knowledge grant.
pub async fn read_native_knowledge_reference_query(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    query: KnowledgeReferenceQuery<'_>,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeReferencesRead> {
    index::check_read(original, admission)?;
    input(query, limits)?;
    let project = admission.project().map_err(KnowledgeIndexError::from)?;
    let scope = project.scope.live;
    let corpus = index::local_corpus(scope, project.version.version(), original, limits.native)?;
    let StoreOwner::Project { project_id } = scope.owner() else {
        return Err(KnowledgeIndexError::WrongAdmission.into());
    };
    let selected = project.version.lance_branch();
    let uri = corpus.uri.clone();
    let Some(guard) = index::capture_index(scope, &corpus, original, limits.native, || {
        index::check_read(original, admission)
    })?
    else {
        index::check_read(original, admission)?;
        if query.outgoing_cursor.is_some() || query.incoming_cursor.is_some() {
            return Err(KnowledgeReferenceQueryError::InvalidCursor);
        }
        return Ok(KnowledgeReferencesRead::NoCorpus);
    };
    let before = || {
        index::check_read(original, admission)?;
        guard.revalidate(original)?;
        Ok(())
    };
    before()?;
    let pin = crate::lance_branch_store::BranchSnapshot::capture_observed(
        &uri,
        (!selected.is_empty()).then_some(selected.as_str()),
        None,
    )
    .await
    .map_err(KnowledgeIndexError::from)?;
    index::semantic::dimensions(pin.schema())?;
    before()?;
    let generation = index::generation(&uri, &pin);
    let b = binding(
        &generation,
        Selection::Native {
            project_id: *project_id,
            selected_lance_branch: selected.as_str().to_owned(),
        },
        query,
        limits,
    );
    check_cursors(query, &b)?;
    let read = collect(
        original,
        pin.scan(),
        Some(*project_id),
        query,
        limits,
        b,
        before,
    )
    .await?;
    drop(pin);
    before()?;
    Ok(KnowledgeReferencesRead::Published {
        generation,
        read: Box::new(read),
    })
}

struct Head {
    source: KnowledgeReferenceSource,
    outgoing: usize,
    incoming: usize,
    prefix: usize,
}
type Heads = BTreeMap<String, Head>;
fn matches(target: KnowledgeReferenceTarget<'_>, other: KnowledgeReferenceTarget<'_>) -> bool {
    target.target_kind == other.target_kind
        && target.namespace == other.namespace
        && target.id == other.id
}
fn increment(value: &mut usize) -> std::result::Result<(), KnowledgeIndexError> {
    *value = value
        .checked_add(1)
        .ok_or(KnowledgeIndexError::Overflow("reference_count"))?;
    Ok(())
}
fn observation<'a>(
    heads: &'a Heads,
    target: KnowledgeReferenceTarget<'_>,
) -> Option<&'a KnowledgeReferenceSource> {
    if target.target_kind != KnowledgeReferenceTargetKind::Knowledge {
        return None;
    }
    heads.get(target.id).map(|h| &h.source).filter(|s| {
        s.namespace
            .as_ref()
            .is_some_and(|ns| ns.borrow() == target.namespace)
    })
}
fn target_owned(target: KnowledgeReferenceTarget<'_>) -> KnowledgeReferenceOwnedTarget {
    KnowledgeReferenceOwnedTarget {
        target_kind: target.target_kind,
        namespace: owned_namespace(target.namespace),
        id: target.id.to_owned(),
    }
}
struct Pages {
    outgoing: Vec<KnowledgeReferenceEdge>,
    incoming: Vec<KnowledgeReferenceEdge>,
}
struct Candidate<'a> {
    target: KnowledgeReferenceTarget<'a>,
    relation: Option<&'a str>,
    field: Option<&'a str>,
    ordinal: usize,
    provenance: KnowledgeReferenceProvenance,
}
fn retain(
    pages: &mut Pages,
    heads: &Heads,
    source: &KnowledgeReferenceSource,
    edge: &Candidate<'_>,
    direction: Direction,
) {
    let known = observation(heads, edge.target);
    let target = target_owned(edge.target);
    let payload = match edge.provenance {
        KnowledgeReferenceProvenance::Authored => KnowledgeReferencePayload::Authored {
            target,
            relation: edge
                .relation
                .expect("authored candidate has relation")
                .to_owned(),
            field: edge.field.map(str::to_owned),
        },
        KnowledgeReferenceProvenance::WikiLink => KnowledgeReferencePayload::WikiLink { target },
    };
    let result = KnowledgeReferenceEdge {
        source: source.clone(),
        payload,
        provenance: edge.provenance,
        ordinal: edge.ordinal,
        target_observation: known.map_or(KnowledgeReferenceObservation::Unresolved, |s| {
            KnowledgeReferenceObservation::KnowledgeHead {
                slug: s.slug.clone(),
                title: s.title.clone(),
            }
        }),
    };
    match direction {
        Direction::Outgoing => pages.outgoing.push(result),
        Direction::Incoming => pages.incoming.push(result),
    };
}
fn in_page(index: usize, offset: usize, size: usize) -> bool {
    index >= offset && index - offset < size
}
fn candidates(
    fields: refs::KnowledgeReferenceFields<'_>,
    wiki: &StringArray,
    mut visit: impl FnMut(Candidate<'_>) -> std::result::Result<(), KnowledgeIndexError>,
) -> std::result::Result<(), KnowledgeIndexError> {
    for (ordinal, r) in fields.authored.iter().enumerate() {
        visit(Candidate {
            target: KnowledgeReferenceTarget {
                target_kind: r.target_kind,
                namespace: KnowledgeReferenceNamespace {
                    scope: r.scope,
                    context: r.context,
                },
                id: r.id,
            },
            relation: Some(r.relation),
            field: r.field,
            ordinal,
            provenance: KnowledgeReferenceProvenance::Authored,
        })?;
    }
    for (ordinal, id) in wiki.iter().enumerate() {
        let id = id.ok_or(KnowledgeIndexError::InvalidField("target_null"))?;
        if id.is_empty() {
            continue;
        }
        visit(Candidate {
            target: KnowledgeReferenceTarget {
                target_kind: KnowledgeReferenceTargetKind::Knowledge,
                namespace: fields.namespace,
                id,
            },
            relation: None,
            field: None,
            ordinal,
            provenance: KnowledgeReferenceProvenance::WikiLink,
        })?;
    }
    Ok(())
}

struct Row<'a> {
    slug: &'a str,
    title: &'a str,
    path: &'a str,
    digest: &'a [u8; 32],
    wiki: Arc<dyn Array>,
    state: u8,
    projection: &'a StructArray,
    index: usize,
}
fn rows(
    batch: &RecordBatch,
    limits: KnowledgeIndexReadLimits,
    mut visit: impl FnMut(Row<'_>) -> std::result::Result<(), KnowledgeIndexError>,
) -> std::result::Result<(), KnowledgeIndexError> {
    if batch.schema().as_ref() != index::head_schema().as_ref() {
        return Err(KnowledgeIndexError::InvalidField("head_schema"));
    }
    if batch.num_rows() > limits.batch_rows.get() as usize {
        return Err(KnowledgeIndexError::LimitExceeded("batch_rows"));
    }
    for (f, c) in batch.schema().fields().iter().zip(batch.columns()) {
        if !f.is_nullable() && c.null_count() != 0 {
            return Err(KnowledgeIndexError::InvalidField("null_required"));
        }
    }
    let slugs: &StringArray = index::array(batch, 0)?;
    let titles: &StringArray = index::array(batch, 1)?;
    let times: &TimestampNanosecondArray = index::array(batch, 2)?;
    let paths: &StringArray = index::array(batch, 3)?;
    let digests: &FixedSizeBinaryArray = index::array(batch, 4)?;
    let wiki: &ListArray = index::array(batch, 5)?;
    let states: &UInt8Array = index::array(batch, 6)?;
    let projection: &StructArray = index::array(batch, 7)?;
    for i in 0..batch.num_rows() {
        let slug = slugs.value(i);
        index::validate_slug(slug)?;
        if slug.len() > limits.content.max_identifier_bytes.get() {
            return Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"));
        }
        if !times.is_null(i) {
            index::time(times.value(i))?;
        }
        if !projection.is_null(i) && !index::logical_path(paths.value(i)) {
            return Err(KnowledgeIndexError::InvalidField("source_path"));
        }
        let targets = wiki.value(i);
        let targets_ref = targets
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(KnowledgeIndexError::InvalidField("targets"))?;
        if targets_ref.null_count() != 0 {
            return Err(KnowledgeIndexError::InvalidField("target_null"));
        }
        for text in targets_ref.iter().flatten() {
            if text.len() > limits.content.max_identifier_bytes.get() {
                return Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"));
            }
        }
        visit(Row {
            slug,
            title: titles.value(i),
            path: paths.value(i),
            digest: digests
                .value(i)
                .try_into()
                .map_err(|_| KnowledgeIndexError::InvalidField("compilation_digest"))?,
            wiki: targets,
            state: states.value(i),
            projection,
            index: i,
        })?;
    }
    Ok(())
}

async fn collect(
    original: &Arc<SnapshotContext>,
    mut scanner: Scanner,
    expected_project: Option<ProjectId>,
    query: KnowledgeReferenceQuery<'_>,
    limits: KnowledgeIndexReadLimits,
    binding: Binding,
    mut before: impl FnMut() -> std::result::Result<(), KnowledgeIndexError>,
) -> Result<KnowledgeReferenceSnapshotRead> {
    let mut check = || -> std::result::Result<(), KnowledgeIndexError> {
        original.check()?;
        before()?;
        original.check()?;
        Ok(())
    };
    check()?;
    scanner
        .project(&[
            "slug",
            "title",
            "updated_at",
            "source_path",
            "compilation_digest",
            "resolved_wiki_targets",
            "document_state",
            "reference_projection",
        ])
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    let mut heads = Heads::new();
    let mut pages = Pages {
        outgoing: Vec::new(),
        incoming: Vec::new(),
    };
    let outgoing_offset = query.outgoing_cursor.map_or(0, |c| c.offset);
    let incoming_offset = query.incoming_cursor.map_or(0, |c| c.offset);
    for pass in 0..2 {
        check()?;
        let mut stream = scanner.try_into_stream().await.map_err(StoreError::from)?;
        check()?;
        let mut seen = BTreeSet::new();
        loop {
            check()?;
            let next = stream.try_next().await.map_err(StoreError::from)?;
            check()?;
            let Some(batch) = next else { break };
            // Borrowed metadata is decoded once; only selected output strings are copied.
            rows(&batch, limits, |row| {
                check()?;
                if !seen.insert(row.slug.to_owned()) {
                    return Err(KnowledgeIndexError::DuplicateSlug);
                }
                let wiki = row
                    .wiki
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or(KnowledgeIndexError::InvalidField("targets"))?;
                let mut projected = false;
                let mut witness = None;
                // The visitor borrows its validated temporary tuples; no full edge list survives.
                let state = refs::visit_row(
                    refs::ReferenceRowInput {
                        projection: row.projection,
                        state: row.state,
                        row: row.index,
                        header: refs::WitnessHeader {
                            slug: row.slug,
                            title: row.title,
                            source_path: row.path,
                            digest: row.digest,
                        },
                        wiki,
                        expected_project,
                    },
                    Some(limits.content.max_identifier_bytes),
                    || {
                        original.check()?;
                        Ok(())
                    },
                    |fields, hash| {
                        if fields.state == KnowledgeDocumentState::Removed {
                            return Ok(());
                        }
                        witness = Some(hash);
                        projected = true;
                        if pass == 0 {
                            let mut outgoing = 0;
                            let mut incoming = 0;
                            candidates(fields, wiki, |e| {
                                original.check()?;
                                increment(&mut outgoing)?;
                                if query.incoming.is_some_and(|t| matches(t, e.target)) {
                                    increment(&mut incoming)?;
                                }
                                Ok(())
                            })?;
                            heads.insert(
                                row.slug.to_owned(),
                                Head {
                                    source: KnowledgeReferenceSource {
                                        slug: row.slug.to_owned(),
                                        title: row.title.to_owned(),
                                        source_path: row.path.to_owned(),
                                        compilation_digest: CompilationDigest32::from_bytes(
                                            *row.digest,
                                        ),
                                        namespace: Some(owned_namespace(fields.namespace)),
                                        witness,
                                    },
                                    outgoing,
                                    incoming,
                                    prefix: 0,
                                },
                            );
                        } else {
                            let head = heads
                                .get(row.slug)
                                .ok_or(KnowledgeIndexError::InvalidField("reference_snapshot"))?;
                            if head.source.witness != witness
                                || head
                                    .source
                                    .namespace
                                    .as_ref()
                                    .map(KnowledgeReferenceOwnedNamespace::borrow)
                                    != Some(fields.namespace)
                            {
                                return Err(KnowledgeIndexError::InvalidField(
                                    "reference_snapshot",
                                ));
                            }
                            let mut outgoing = 0;
                            let mut incoming = 0;
                            candidates(fields, wiki, |e| {
                                original.check()?;
                                if query.outgoing == Some(row.slug)
                                    && in_page(outgoing, outgoing_offset, query.page_size.get())
                                {
                                    retain(
                                        &mut pages,
                                        &heads,
                                        &head.source,
                                        &e,
                                        Direction::Outgoing,
                                    );
                                }
                                increment(&mut outgoing)?;
                                if query.incoming.is_some_and(|t| matches(t, e.target)) {
                                    let position = head
                                        .prefix
                                        .checked_add(incoming)
                                        .ok_or(KnowledgeIndexError::Overflow("reference_count"))?;
                                    if in_page(position, incoming_offset, query.page_size.get()) {
                                        retain(
                                            &mut pages,
                                            &heads,
                                            &head.source,
                                            &e,
                                            Direction::Incoming,
                                        );
                                    }
                                    increment(&mut incoming)?;
                                }
                                Ok(())
                            })?;
                            if outgoing != head.outgoing || incoming != head.incoming {
                                return Err(KnowledgeIndexError::InvalidField(
                                    "reference_snapshot",
                                ));
                            }
                        }
                        Ok(())
                    },
                )?;
                if state == KnowledgeDocumentState::Active && !projected && pass == 0 {
                    heads.insert(
                        row.slug.to_owned(),
                        Head {
                            source: KnowledgeReferenceSource {
                                slug: row.slug.to_owned(),
                                title: row.title.to_owned(),
                                source_path: row.path.to_owned(),
                                compilation_digest: CompilationDigest32::from_bytes(*row.digest),
                                namespace: None,
                                witness: None,
                            },
                            outgoing: 0,
                            incoming: 0,
                            prefix: 0,
                        },
                    );
                }
                check()?;
                Ok(())
            })?;
        }
        drop(stream);
        check()?;
        if pass == 0 {
            if query.outgoing.is_some_and(|slug| !heads.contains_key(slug)) {
                return Err(KnowledgeReferenceQueryError::UnknownSource);
            }
            let mut prefix = 0;
            for head in heads.values_mut() {
                check()?;
                head.prefix = prefix;
                prefix = prefix
                    .checked_add(head.incoming)
                    .ok_or(KnowledgeIndexError::Overflow("reference_count"))?;
            }
            let outgoing_total = query
                .outgoing
                .and_then(|slug| heads.get(slug))
                .map_or(0, |h| h.outgoing);
            if outgoing_offset > outgoing_total || incoming_offset > prefix {
                return Err(KnowledgeReferenceQueryError::InvalidCursor);
            }
        }
    }
    drop(scanner);
    check()?;
    pages.outgoing.sort_unstable_by(edge_order);
    pages.incoming.sort_unstable_by(edge_order);
    let mut warnings = Vec::new();
    for head in heads.values() {
        check()?;
        if head.source.witness.is_none() {
            warnings.push(KnowledgeReferencesWarning::MissingCurrentProjection {
                source: head.source.clone(),
            });
        }
    }
    let complete = warnings.is_empty();
    let incoming_total = heads.values().try_fold(0usize, |n, h| {
        n.checked_add(h.incoming)
            .ok_or(KnowledgeIndexError::Overflow("reference_count"))
    })?;
    let make_page = |results: Vec<KnowledgeReferenceEdge>,
                     total: usize,
                     complete: bool,
                     offset: usize,
                     direction: Direction|
     -> Result<Box<KnowledgeReferencePage>> {
        let next = offset
            .checked_add(results.len())
            .ok_or(KnowledgeIndexError::Overflow("reference_count"))?;
        let next_cursor = if next < total {
            Some(KnowledgeReferencesCursor {
                binding: Box::new(binding.clone()),
                direction,
                offset: next,
            })
        } else {
            None
        };
        Ok(Box::new(KnowledgeReferencePage {
            results,
            total,
            complete,
            next_cursor,
        }))
    };
    let outgoing = if let Some(slug) = query.outgoing {
        let h = &heads[slug];
        Some(make_page(
            std::mem::take(&mut pages.outgoing),
            h.outgoing,
            h.source.witness.is_some(),
            outgoing_offset,
            Direction::Outgoing,
        )?)
    } else {
        None
    };
    let incoming = if query.incoming.is_some() {
        Some(make_page(
            std::mem::take(&mut pages.incoming),
            incoming_total,
            complete,
            incoming_offset,
            Direction::Incoming,
        )?)
    } else {
        None
    };
    let focused_source = query
        .outgoing
        .map(|slug| {
            heads
                .remove(slug)
                .map(|head| head.source)
                .ok_or(KnowledgeReferenceQueryError::UnknownSource)
        })
        .transpose()?;
    check()?;
    Ok(KnowledgeReferenceSnapshotRead {
        focused_source,
        outgoing,
        incoming,
        coverage: KnowledgeReferencesCoverage {
            knowledge_source_complete: complete,
            inbound_other_sources_not_qualified: true,
        },
        warnings,
    })
}
fn edge_order(a: &KnowledgeReferenceEdge, b: &KnowledgeReferenceEdge) -> std::cmp::Ordering {
    a.source
        .slug
        .cmp(&b.source.slug)
        .then_with(|| (a.provenance as u8).cmp(&(b.provenance as u8)))
        .then_with(|| a.ordinal.cmp(&b.ordinal))
}
