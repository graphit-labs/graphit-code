//! Native Knowledge provenance queries over one actual SDK pin.
//! Metadata observations and closed continuations never grant access.
use crate::{
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
    Array, ListArray, RecordBatch, StringArray, StructArray, TimestampNanosecondArray, UInt8Array,
};
use futures::TryStreamExt;
use graphit_auth::ProjectReadAdmission;
use graphit_core::{
    artifact_store::StoreOwner, file_io::SnapshotContext, project_identity::ProjectId,
};
use lance::dataset::scanner::{ColumnOrdering, Scanner};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, error::Error, fmt, num::NonZeroUsize, sync::Arc};

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
impl KnowledgeReferenceOwnedScope {}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeReferenceOwnedNamespace {
    pub scope: KnowledgeReferenceOwnedScope,
    pub context: Option<String>,
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
    pub namespace: Option<KnowledgeReferenceOwnedNamespace>,
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
/// A continuation is scoped to the admitted publication and requested focus.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeReferencesCursor {
    binding: String,
    direction: Direction,
    offset: usize,
}

pub(crate) fn owned_namespace(
    ns: KnowledgeReferenceNamespace<'_>,
) -> KnowledgeReferenceOwnedNamespace {
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
    }
    Ok(())
}
fn binding(
    generation: &KnowledgeGeneration,
    selection: Selection,
    query: KnowledgeReferenceQuery<'_>,
) -> Result<String> {
    let key = (
        &generation.corpus_uri,
        generation.version,
        &generation.physical_branch,
        &generation.inherited_target,
        selection,
        query.outgoing,
        query.incoming.map(target_owned),
    );
    let bytes = serde_json::to_vec(&key).map_err(KnowledgeIndexError::ReferenceSerialization)?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
fn check_cursors(query: KnowledgeReferenceQuery<'_>, binding: &str) -> Result<()> {
    for c in [query.outgoing_cursor, query.incoming_cursor]
        .into_iter()
        .flatten()
    {
        if c.binding != binding {
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
    let b = binding(generation, Selection::CallerPin, query)?;
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
    let scope = project.scope;
    let corpus = index::local_corpus(scope)?;
    let StoreOwner::Project { project_id } = scope.owner() else {
        return Err(KnowledgeIndexError::WrongAdmission.into());
    };
    let selected = project.version.lance_branch();
    let uri = corpus.uri.clone();
    if !graphit_core::local_files::directory_exists(&corpus.path)
        .map_err(KnowledgeIndexError::from)?
    {
        index::check_read(original, admission)?;
        if query.outgoing_cursor.is_some() || query.incoming_cursor.is_some() {
            return Err(KnowledgeReferenceQueryError::InvalidCursor);
        }
        return Ok(KnowledgeReferencesRead::NoCorpus);
    };
    let before = || {
        index::check_read(original, admission)?;

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
    )?;
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
fn observe_target(
    heads: &BTreeMap<String, KnowledgeReferenceSource>,
    edge: &mut KnowledgeReferenceEdge,
) {
    let target = match &edge.payload {
        KnowledgeReferencePayload::Authored { target, .. }
        | KnowledgeReferencePayload::WikiLink { target } => target,
    };
    if target.target_kind == KnowledgeReferenceTargetKind::Knowledge
        && let Some(source) = heads.get(&target.id)
        && source.namespace.as_ref() == Some(&target.namespace)
    {
        edge.target_observation = KnowledgeReferenceObservation::KnowledgeHead {
            slug: source.slug.clone(),
            title: source.title.clone(),
        };
    }
}
fn target_owned(target: KnowledgeReferenceTarget<'_>) -> KnowledgeReferenceOwnedTarget {
    KnowledgeReferenceOwnedTarget {
        target_kind: target.target_kind,
        namespace: owned_namespace(target.namespace),
        id: target.id.to_owned(),
    }
}
struct Candidate<'a> {
    target: KnowledgeReferenceTarget<'a>,
    relation: Option<&'a str>,
    field: Option<&'a str>,
    ordinal: usize,
    provenance: KnowledgeReferenceProvenance,
}
fn materialize(source: &KnowledgeReferenceSource, edge: &Candidate<'_>) -> KnowledgeReferenceEdge {
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
    KnowledgeReferenceEdge {
        source: source.clone(),
        payload,
        provenance: edge.provenance,
        ordinal: edge.ordinal,
        target_observation: KnowledgeReferenceObservation::Unresolved,
    }
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
    if index::without_legacy_reference_field(batch.schema().as_ref()).as_ref()
        != index::head_schema().as_ref()
    {
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
    let wiki: &ListArray = index::array(batch, 4)?;
    let states: &UInt8Array = index::array(batch, 5)?;
    let projection: &StructArray = index::array(batch, 6)?;
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
    binding: String,
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
        .project(&index::HEAD_COLUMNS)
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    scanner
        .order_by(Some(vec![ColumnOrdering::asc_nulls_first(
            "slug".to_owned(),
        )]))
        .map_err(StoreError::from)?;
    let mut heads = BTreeMap::new();
    let mut outgoing_edges = Vec::new();
    let mut incoming_edges = Vec::new();
    let outgoing_offset = query.outgoing_cursor.map_or(0, |c| c.offset);
    let incoming_offset = query.incoming_cursor.map_or(0, |c| c.offset);
    let mut outgoing_total = 0;
    let mut incoming_total = 0;
    let mut previous_slug = String::new();
    let mut stream = scanner.try_into_stream().await.map_err(StoreError::from)?;
    loop {
        check()?;
        let Some(batch) = stream.try_next().await.map_err(StoreError::from)? else {
            break;
        };
        rows(&batch, limits, |row| {
            check()?;
            if previous_slug == row.slug {
                return Err(KnowledgeIndexError::DuplicateSlug);
            }
            row.slug.clone_into(&mut previous_slug);
            let wiki = row
                .wiki
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or(KnowledgeIndexError::InvalidField("targets"))?;
            let mut source = KnowledgeReferenceSource {
                slug: row.slug.to_owned(),
                title: row.title.to_owned(),
                namespace: None,
            };
            let state = refs::visit_row(
                refs::ReferenceRowInput {
                    projection: row.projection,
                    state: row.state,
                    row: row.index,
                    slug: row.slug,
                    wiki_count: wiki.len(),
                    expected_project,
                },
                Some(limits.content.max_identifier_bytes),
                || {
                    original.check()?;
                    Ok(())
                },
                |fields| {
                    if fields.state == KnowledgeDocumentState::Removed {
                        return Ok(());
                    }
                    source.namespace = Some(owned_namespace(fields.namespace));
                    candidates(fields, wiki, |edge| {
                        original.check()?;
                        if query.outgoing == Some(row.slug) {
                            if in_page(outgoing_total, outgoing_offset, query.page_size.get()) {
                                outgoing_edges.push(materialize(&source, &edge));
                            }
                            increment(&mut outgoing_total)?;
                        }
                        if query
                            .incoming
                            .is_some_and(|target| matches(target, edge.target))
                        {
                            if in_page(incoming_total, incoming_offset, query.page_size.get()) {
                                incoming_edges.push(materialize(&source, &edge));
                            }
                            increment(&mut incoming_total)?;
                        }
                        Ok(())
                    })
                },
            )?;
            if state == KnowledgeDocumentState::Active {
                heads.insert(row.slug.to_owned(), source);
            }
            Ok(())
        })?;
    }
    if query.outgoing.is_some_and(|slug| !heads.contains_key(slug)) {
        return Err(KnowledgeReferenceQueryError::UnknownSource);
    }
    if outgoing_offset > outgoing_total || incoming_offset > incoming_total {
        return Err(KnowledgeReferenceQueryError::InvalidCursor);
    }
    for edge in outgoing_edges.iter_mut().chain(&mut incoming_edges) {
        check()?;
        observe_target(&heads, edge);
    }
    let warnings: Vec<_> = heads
        .values()
        .filter(|source| source.namespace.is_none())
        .map(
            |source| KnowledgeReferencesWarning::MissingCurrentProjection {
                source: source.clone(),
            },
        )
        .collect();
    let complete = warnings.is_empty();
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
                binding: binding.clone(),
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
        Some(make_page(
            outgoing_edges,
            outgoing_total,
            heads[slug].namespace.is_some(),
            outgoing_offset,
            Direction::Outgoing,
        )?)
    } else {
        None
    };
    let incoming = if query.incoming.is_some() {
        Some(make_page(
            incoming_edges,
            incoming_total,
            complete,
            incoming_offset,
            Direction::Incoming,
        )?)
    } else {
        None
    };
    let focused_source = query.outgoing.and_then(|slug| heads.remove(slug));
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
