//! Typed authored state and immutable generations in the actual Task corpus.
use super::{TaskStoreError, TaskStoreReadLimits, model::*};
use crate::{reference_projection as kernel, task_spec::TaskId};
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskReferenceKind {
    Task,
    Session,
    Memory,
    Knowledge,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskReferenceScope {
    Project(ProjectId),
    User(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReference {
    pub target_kind: TaskReferenceKind,
    pub scope: TaskReferenceScope,
    pub id: String,
    pub context: Option<String>,
    pub relation: String,
    pub field: Option<String>,
}

/// Normalized request identity; omission and explicit clearing remain distinct.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskReferenceUpdate {
    Preserve,
    Replace(Vec<TaskReference>),
}
impl TaskReferenceUpdate {
    pub(super) fn apply(&self, current: &mut Vec<TaskReference>) {
        if let Self::Replace(next) = self {
            current.clone_from(next);
        }
    }
    pub(super) fn initial(&self) -> Vec<TaskReference> {
        match self {
            Self::Preserve => Vec::new(),
            Self::Replace(r) => r.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TaskReferencesInputLimits {
    pub max_entries: NonZeroUsize,
    pub max_text_bytes: NonZeroUsize,
}

pub(super) fn authored(
    original: &SnapshotContext,
    project_id: ProjectId,
    field: kernel::AuthoredReferencesField<'_, '_>,
    limits: TaskReferencesInputLimits,
) -> Result<TaskReferenceUpdate, TaskStoreError> {
    original.check()?;
    if let kernel::AuthoredReferencesField::Sequence(entries) = field {
        if entries.len() > limits.max_entries.get() {
            return Err(TaskStoreError::Invalid("reference input entry limit"));
        }
        let mut bytes = 0usize;
        for entry in entries {
            original.check()?;
            if let kernel::AuthoredReferenceEntry::Tuple(c) = entry {
                for text in [
                    Some(c.target_type),
                    Some(c.id),
                    c.scope,
                    c.scope_id,
                    c.context,
                    Some(c.relation),
                    c.field,
                ]
                .into_iter()
                .flatten()
                {
                    bytes = bytes
                        .checked_add(text.len())
                        .filter(|n| *n <= limits.max_text_bytes.get())
                        .ok_or(TaskStoreError::Invalid("reference input text byte limit"))?;
                }
            }
        }
    }
    let update = kernel::project_authored_references(
        original,
        kernel::SourceNamespace {
            scope: kernel::LogicalScope::Project(project_id),
            context: None,
        },
        field,
    )
    .map_err(|e| match e {
        kernel::ReferenceProjectionError::Request(cause) => TaskStoreError::Request(cause),
        other => TaskStoreError::Reference(other),
    })?;
    let owned = match update {
        kernel::AuthoredReferenceUpdate::Preserve => TaskReferenceUpdate::Preserve,
        kernel::AuthoredReferenceUpdate::Replace(entries) => TaskReferenceUpdate::Replace(
            entries
                .into_iter()
                .map(|r| TaskReference {
                    target_kind: match r.target_kind {
                        kernel::ReferenceTargetKind::Task => TaskReferenceKind::Task,
                        kernel::ReferenceTargetKind::Session => TaskReferenceKind::Session,
                        kernel::ReferenceTargetKind::Memory => TaskReferenceKind::Memory,
                        kernel::ReferenceTargetKind::Knowledge => TaskReferenceKind::Knowledge,
                    },
                    scope: match r.scope {
                        kernel::LogicalScope::Project(id) => TaskReferenceScope::Project(id),
                        kernel::LogicalScope::User(id) => TaskReferenceScope::User(id.to_owned()),
                    },
                    id: r.id.to_owned(),
                    context: r.context.map(str::to_owned),
                    relation: r.relation.to_owned(),
                    field: r.field.map(str::to_owned),
                })
                .collect(),
        ),
    };
    original.check()?;
    Ok(owned)
}

pub(super) fn validate_authored(
    original: &SnapshotContext,
    references: &[TaskReference],
) -> Result<(), TaskStoreError> {
    for r in references {
        original.check()?;
        if r.id.is_empty()
            || r.relation.is_empty()
            || matches!(&r.scope, TaskReferenceScope::User(s) if s.is_empty())
            || r.context.as_deref() == Some("")
            || r.context.is_some() && r.target_kind != TaskReferenceKind::Knowledge
        {
            return Err(TaskStoreError::Invalid("authored reference state"));
        }
    }
    original.check()?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskReferenceRecord {
    Task(TaskId),
    Session(SessionId),
}
impl TaskReferenceRecord {
    pub(super) fn identifier(self) -> String {
        match self {
            Self::Task(t) => id(t.get()),
            Self::Session(s) => id(s.get()),
        }
    }
    pub(super) fn kind(self) -> TaskReferenceKind {
        match self {
            Self::Task(_) => TaskReferenceKind::Task,
            Self::Session(_) => TaskReferenceKind::Session,
        }
    }
    pub(super) fn order(self) -> (u8, String) {
        (
            match self {
                Self::Task(_) => 0,
                Self::Session(_) => 1,
            },
            self.identifier(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskReferenceProvenance {
    Structural,
    Authored,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectedReference {
    pub reference: TaskReference,
    pub provenance: TaskReferenceProvenance,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReferenceProjection {
    pub source: TaskReferenceRecord,
    pub project_id: ProjectId,
    pub project_version: String,
    pub revision: u64,
    pub reference_generation: u64,
    pub edges: Vec<ProjectedReference>,
}
impl ReferenceProjection {
    pub(super) fn key(&self) -> String {
        let (kind, id) = self.source.order();
        format!("reference_projection/{kind}/{id}/{}", self.revision)
    }
    pub(super) fn validate(&self, original: &SnapshotContext) -> Result<(), TaskStoreError> {
        original.check()?;
        if self.revision == 0 || self.reference_generation != self.revision {
            return Err(TaskStoreError::Invalid("reference generation"));
        }
        super::canonical(&self.project_version)?;
        let mut authored_seen = false;
        let mut structural_count = 0usize;
        let mut dependencies_seen = false;
        let mut parent_seen = false;
        for edge in &self.edges {
            validate_authored(original, std::slice::from_ref(&edge.reference))?;
            match edge.provenance {
                TaskReferenceProvenance::Authored => authored_seen = true,
                TaskReferenceProvenance::Structural => {
                    let r = &edge.reference;
                    if authored_seen
                        || matches!(self.source, TaskReferenceRecord::Session(_))
                        || r.scope != TaskReferenceScope::Project(self.project_id)
                        || r.context.is_some()
                        || r.field.is_some()
                        || !matches!(
                            (r.relation.as_str(), r.target_kind),
                            ("session", TaskReferenceKind::Session)
                                | ("parent" | "dependency", TaskReferenceKind::Task)
                        )
                    {
                        return Err(TaskStoreError::Invalid("structural reference"));
                    }
                    match (r.relation.as_str(), r.target_kind) {
                        ("session", TaskReferenceKind::Session) if structural_count == 0 => {
                            super::read::identifier::<SessionId>(&r.id)?;
                        }
                        ("parent", TaskReferenceKind::Task)
                            if structural_count > 0 && !dependencies_seen && !parent_seen =>
                        {
                            super::read::identifier::<TaskId>(&r.id)?;
                            parent_seen = true;
                        }
                        ("dependency", TaskReferenceKind::Task) if structural_count > 0 => {
                            super::read::identifier::<TaskId>(&r.id)?;
                            dependencies_seen = true;
                        }
                        _ => return Err(TaskStoreError::Invalid("structural reference order")),
                    }
                    structural_count += 1;
                }
            }
        }
        if matches!(self.source, TaskReferenceRecord::Task(_)) && structural_count == 0 {
            return Err(TaskStoreError::Invalid(
                "missing structural session reference",
            ));
        }
        original.check()?;
        Ok(())
    }
}

pub(super) fn task_projection(
    task: &Task,
    project_id: ProjectId,
    version: &str,
) -> ReferenceProjection {
    let mut edges = Vec::new();
    let mut structural = |kind, target, relation: &str| {
        edges.push(ProjectedReference {
            provenance: TaskReferenceProvenance::Structural,
            reference: TaskReference {
                target_kind: kind,
                scope: TaskReferenceScope::Project(project_id),
                id: target,
                context: None,
                relation: relation.to_owned(),
                field: None,
            },
        })
    };
    structural(
        TaskReferenceKind::Session,
        id(task.session_id.get()),
        "session",
    );
    if let Some(parent) = task.parent_id {
        structural(TaskReferenceKind::Task, id(parent.get()), "parent");
    }
    for dependency in &task.depends_on {
        structural(TaskReferenceKind::Task, id(dependency.get()), "dependency");
    }
    edges.extend(
        task.references
            .iter()
            .cloned()
            .map(|reference| ProjectedReference {
                reference,
                provenance: TaskReferenceProvenance::Authored,
            }),
    );
    ReferenceProjection {
        source: TaskReferenceRecord::Task(task.specification.id),
        project_id,
        project_version: version.to_owned(),
        revision: task.specification.revision,
        reference_generation: task.reference_generation,
        edges,
    }
}
pub(super) fn session_projection(
    session: &Session,
    project_id: ProjectId,
    version: &str,
) -> ReferenceProjection {
    ReferenceProjection {
        source: TaskReferenceRecord::Session(session.id),
        project_id,
        project_version: version.to_owned(),
        revision: session.revision,
        reference_generation: session.reference_generation,
        edges: session
            .references
            .iter()
            .cloned()
            .map(|reference| ProjectedReference {
                reference,
                provenance: TaskReferenceProvenance::Authored,
            })
            .collect(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReferencesQuery {
    pub focus: TaskReferenceRecord,
    pub page_size: NonZeroUsize,
    pub outgoing_cursor: Option<TaskReferencesCursor>,
    pub incoming_cursor: Option<TaskReferencesCursor>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Direction {
    Outgoing,
    Incoming,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferencePin {
    version: u64,
    physical_branch: Option<String>,
    inherited_target: Option<String>,
}
impl From<&TaskGeneration> for ReferencePin {
    fn from(g: &TaskGeneration) -> Self {
        Self {
            version: g.version,
            physical_branch: g.physical_branch.clone(),
            inherited_target: g.inherited_target.clone(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceBinding {
    corpus_key: String,
    root: graphit_core::native_path::NativePath,
    project_id: ProjectId,
    project_version: String,
    pin: ReferencePin,
    focus: TaskReferenceRecord,
}
/// Closed continuation data for this exact observed pin; it grants no access.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReferencesCursor {
    binding: Box<ReferenceBinding>,
    direction: Direction,
    offset: usize,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskReferenceResolution {
    Task {
        id: TaskId,
        title: String,
        status: crate::gates::TaskStatus,
    },
    Session {
        id: SessionId,
        title: String,
        status: crate::gates::TaskStatus,
    },
    Unresolved,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReferenceEdge {
    pub source: TaskReferenceRecord,
    pub reference: TaskReference,
    pub provenance: TaskReferenceProvenance,
    pub ordinal: usize,
    pub source_revision: u64,
    pub reference_generation: u64,
    pub source_snapshot: TaskGeneration,
    pub source_observation: TaskReferenceResolution,
    pub target_resolution: TaskReferenceResolution,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReferencePage {
    pub results: Vec<TaskReferenceEdge>,
    pub total: usize,
    pub complete: bool,
    pub next_cursor: Option<TaskReferencesCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReferencesCoverage {
    pub project_id: ProjectId,
    pub source_kinds: Vec<TaskReferenceKind>,
    pub task_session_incoming_complete: bool,
    pub inbound_other_sources_not_qualified: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskReferencesWarning {
    MissingCurrentProjection {
        source: TaskReferenceRecord,
        revision: u64,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskReferencesRead {
    NoCorpus,
    Published {
        generation: TaskGeneration,
        focus_revision: u64,
        outgoing: Box<TaskReferencePage>,
        incoming: Box<TaskReferencePage>,
        coverage: TaskReferencesCoverage,
        warnings: Vec<TaskReferencesWarning>,
    },
}

// Retain only current-head observations and their expected reference projections.
// Descriptions, checks and history are dropped in the first scan; a validated
// marker consumes its expected projection before pagination.
struct ReferenceHead {
    revision: u64,
    observation: TaskReferenceResolution,
    expected: Option<ReferenceProjection>,
    marker: bool,
    outgoing: usize,
    incoming: usize,
}
impl ReferenceHead {
    fn observe(&mut self, projection: &ReferenceProjection) -> Result<(), TaskStoreError> {
        let Some(mut expected) = self.expected.take() else {
            return Err(TaskStoreError::Invalid(
                "duplicate current reference marker",
            ));
        };
        // Publication labels describe origin, not authorization. An inherited
        // marker may retain its original version while this pin selects a fork.
        expected
            .project_version
            .clone_from(&projection.project_version);
        if expected != *projection {
            return Err(TaskStoreError::Invalid(
                "current reference marker disagrees with head",
            ));
        }
        self.marker = true;
        Ok(())
    }
}
type Heads = std::collections::BTreeMap<(u8, String), (TaskReferenceRecord, ReferenceHead)>;
fn validate_reference_batch(batch: &arrow_array::RecordBatch) -> Result<(), TaskStoreError> {
    super::rows::validate_schema(batch.schema().as_ref())
}

async fn reference_scan(
    pin: &graphit_data::lance_branch_store::BranchSnapshot,
    filter: &str,
    original: &SnapshotContext,
    limits: TaskStoreReadLimits,
    before: &impl Fn() -> Result<(), TaskStoreError>,
    mut each: impl FnMut(super::rows::Row) -> Result<(), TaskStoreError>,
) -> Result<(), TaskStoreError> {
    use futures::TryStreamExt;
    use graphit_data::lance_module_store::StoreError;
    before()?;
    let mut scan = pin.scan();
    scan.filter(filter)
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    let mut stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    loop {
        before()?;
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else { break };
        validate_reference_batch(&batch)?;
        for i in 0..batch.num_rows() {
            before()?;
            each(super::rows::decode(&batch, i, original)?)?;
        }
    }
    before()?;
    Ok(())
}
fn head(
    row: super::rows::Row,
    project_id: ProjectId,
    version: &str,
) -> Result<(TaskReferenceRecord, ReferenceHead), TaskStoreError> {
    let projection = expected(&row, project_id, version)?;
    let (source, revision, observation) = match row {
        super::rows::Row::Task(t) => (
            TaskReferenceRecord::Task(t.record.specification.id),
            t.record.specification.revision,
            TaskReferenceResolution::Task {
                id: t.record.specification.id,
                title: t.record.specification.title,
                status: t.record.status,
            },
        ),
        super::rows::Row::Session(s) => (
            TaskReferenceRecord::Session(s.record.id),
            s.record.revision,
            TaskReferenceResolution::Session {
                id: s.record.id,
                title: s.record.input.title,
                status: s.record.status,
            },
        ),
        _ => return Err(TaskStoreError::Invalid("reference head family")),
    };
    Ok((
        source,
        ReferenceHead {
            revision,
            observation,
            expected: Some(projection),
            marker: false,
            outgoing: 0,
            incoming: 0,
        },
    ))
}
fn expected(
    row: &super::rows::Row,
    project_id: ProjectId,
    version: &str,
) -> Result<ReferenceProjection, TaskStoreError> {
    match row {
        super::rows::Row::Task(t) => Ok(task_projection(&t.record, project_id, version)),
        super::rows::Row::Session(s) => Ok(session_projection(&s.record, project_id, version)),
        _ => Err(TaskStoreError::Invalid("reference head family")),
    }
}
fn targets(reference: &TaskReference, focus: TaskReferenceRecord, project_id: ProjectId) -> bool {
    reference.target_kind == focus.kind()
        && reference.scope == TaskReferenceScope::Project(project_id)
        && reference.context.is_none()
        && reference.id == focus.identifier()
}
fn resolution<'a>(
    r: &TaskReference,
    project_id: ProjectId,
    heads: &'a Heads,
) -> Option<&'a TaskReferenceResolution> {
    if r.scope != TaskReferenceScope::Project(project_id) || r.context.is_some() {
        return None;
    }
    let record = match r.target_kind {
        TaskReferenceKind::Task => super::read::identifier::<TaskId>(&r.id)
            .ok()
            .map(TaskReferenceRecord::Task),
        TaskReferenceKind::Session => super::read::identifier::<SessionId>(&r.id)
            .ok()
            .map(TaskReferenceRecord::Session),
        _ => None,
    };
    record
        .and_then(|id| heads.get(&id.order()))
        .map(|(_, h)| &h.observation)
}
async fn current_projection(
    pin: &graphit_data::lance_branch_store::BranchSnapshot,
    source: TaskReferenceRecord,
    revision: u64,
    original: &SnapshotContext,
    limits: TaskStoreReadLimits,
    before: &impl Fn() -> Result<(), TaskStoreError>,
) -> Result<ReferenceProjection, TaskStoreError> {
    let (kind, id) = source.order();
    let filter = format!(
        "kind = 'reference_projection' AND row_key = 'reference_projection/{kind}/{id}/{revision}'"
    );
    let mut found = None;
    reference_scan(pin, &filter, original, limits, before, |row| {
        let super::rows::Row::ReferenceProjection(p) = row else {
            return Err(TaskStoreError::Invalid("reference marker family"));
        };
        if p.source != source || p.revision != revision || found.replace(p).is_some() {
            return Err(TaskStoreError::Invalid("duplicate reference marker"));
        }
        Ok(())
    })
    .await?;
    found.ok_or(TaskStoreError::Invalid(
        "validated reference marker vanished",
    ))
}

#[derive(Clone, Copy)]
struct PageContext<'a> {
    project_id: ProjectId,
    heads: &'a Heads,
    generation: &'a TaskGeneration,
}

fn page_edge(
    source: TaskReferenceRecord,
    h: &ReferenceHead,
    ordinal: usize,
    p: &ProjectedReference,
    page: &PageContext<'_>,
) -> TaskReferenceEdge {
    let PageContext {
        project_id,
        heads,
        generation,
    } = *page;
    let target = resolution(&p.reference, project_id, heads);
    let r = &p.reference;
    TaskReferenceEdge {
        source,
        reference: r.clone(),
        provenance: p.provenance,
        ordinal,
        source_revision: h.revision,
        reference_generation: h.revision,
        source_snapshot: generation.clone(),
        source_observation: h.observation.clone(),
        target_resolution: target
            .cloned()
            .unwrap_or(TaskReferenceResolution::Unresolved),
    }
}

impl super::NativeTaskStore {
    pub async fn references(
        original: &std::sync::Arc<SnapshotContext>,
        admission: &graphit_auth::ProjectReadAdmission<'_>,
        query: TaskReferencesQuery,
        limits: TaskStoreReadLimits,
    ) -> Result<TaskReferencesRead, TaskStoreError> {
        use super::{native, rows};
        native::check_read(original, admission)?;
        if query.page_size.get() > 100 {
            return Err(TaskStoreError::Invalid("reference page size"));
        }
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let project_version = project.version.version().to_owned();
        let root =
            graphit_core::native_path::NativePath::new(project.scope.live.owner_root().to_owned())?;
        let Some(captured) = native::read_pin(original, admission, limits).await? else {
            return if query.outgoing_cursor.is_some() || query.incoming_cursor.is_some() {
                Err(TaskStoreError::InvalidCursor)
            } else {
                Ok(TaskReferencesRead::NoCorpus)
            };
        };
        let before = || {
            native::check_read(original, admission)?;
            captured.guard.revalidate(original)?;
            Ok(())
        };
        before()?;
        let pin = &captured.pin;
        let generation = TaskGeneration {
            version: pin.version(),
            physical_branch: pin.physical_branch().map(str::to_owned),
            inherited_target: pin.inherited_target().map(str::to_owned),
        };
        let binding = ReferenceBinding {
            corpus_key: captured.guard.corpus_key(pin.corpus_uri()),
            root,
            project_id,
            project_version,
            pin: (&generation).into(),
            focus: query.focus,
        };
        let offset = |cursor: &Option<TaskReferencesCursor>, direction| match cursor {
            None => Ok(0),
            Some(c) if c.binding.as_ref() == &binding && c.direction == direction => Ok(c.offset),
            _ => Err(TaskStoreError::InvalidCursor),
        };
        let outgoing_offset = offset(&query.outgoing_cursor, Direction::Outgoing)?;
        let incoming_offset = offset(&query.incoming_cursor, Direction::Incoming)?;

        let mut heads = Heads::new();
        reference_scan(
            pin,
            "kind IN ('task', 'session')",
            original,
            limits,
            &before,
            |row| {
                let (source, h) = head(row, project_id, &binding.project_version)?;
                if heads.insert(source.order(), (source, h)).is_some() {
                    return Err(TaskStoreError::Invalid("duplicate reference head"));
                }
                Ok(())
            },
        )
        .await?;
        let focus_revision = heads
            .get(&query.focus.order())
            .ok_or(match query.focus {
                TaskReferenceRecord::Task(_) => TaskStoreError::MissingTask,
                TaskReferenceRecord::Session(_) => TaskStoreError::MissingSession,
            })?
            .1
            .revision;
        // Validate every consulted marker, including older/orphan generations.
        // Current markers additionally compare against the actual same-pin head.
        // Stream one batch at a time; nested point reads do not retain history.
        use futures::TryStreamExt;
        use graphit_data::lance_module_store::StoreError;
        let mut scan = pin.scan();
        scan.filter("kind = 'reference_projection'")
            .map_err(StoreError::from)?
            .batch_size(limits.batch_rows.get() as usize);
        before()?;
        let mut stream = crate::storage::read(original, scan.try_into_stream())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        loop {
            before()?;
            let next = crate::storage::read(original, stream.try_next())
                .await?
                .map_err(StoreError::from)?;
            before()?;
            let Some(batch) = next else { break };
            validate_reference_batch(&batch)?;
            for i in 0..batch.num_rows() {
                before()?;
                let rows::Row::ReferenceProjection(p) = rows::decode(&batch, i, original)? else {
                    return Err(TaskStoreError::Invalid("reference marker family"));
                };
                // The captured SDK pin authorizes the selected branch. The
                // canonical publication label may predate an inherited fork.
                if p.project_id != project_id {
                    return Err(TaskStoreError::Invalid("foreign reference marker"));
                }
                let Some((_, h)) = heads.get_mut(&p.source.order()) else {
                    continue;
                };
                if h.revision != p.revision {
                    continue;
                }
                h.observe(&p)?;
                h.outgoing = p.edges.len();
                h.incoming = p
                    .edges
                    .iter()
                    .filter(|e| targets(&e.reference, query.focus, project_id))
                    .count();
            }
        }
        before()?;
        let mut warnings = Vec::new();
        let mut incoming_total = 0usize;
        for (source, h) in heads.values() {
            before()?;
            if !h.marker {
                warnings.push(TaskReferencesWarning::MissingCurrentProjection {
                    source: *source,
                    revision: h.revision,
                });
            }
            incoming_total = incoming_total
                .checked_add(h.incoming)
                .ok_or(TaskStoreError::Invalid("reference total overflow"))?;
        }
        let focus_head = &heads
            .get(&query.focus.order())
            .ok_or(TaskStoreError::MissingTask)?
            .1;
        let outgoing_total = focus_head.outgoing;
        if outgoing_offset > outgoing_total || incoming_offset > incoming_total {
            return Err(TaskStoreError::InvalidCursor);
        }
        let make_page =
            |total: usize, complete: bool, offset: usize, direction: Direction| TaskReferencePage {
                results: Vec::new(),
                total,
                complete,
                next_cursor: (offset.saturating_add(query.page_size.get()) < total).then(|| {
                    TaskReferencesCursor {
                        binding: Box::new(binding.clone()),
                        direction,
                        offset: offset + query.page_size.get(),
                    }
                }),
            };
        let mut outgoing = make_page(
            outgoing_total,
            focus_head.marker,
            outgoing_offset,
            Direction::Outgoing,
        );
        let mut incoming = make_page(
            incoming_total,
            warnings.is_empty(),
            incoming_offset,
            Direction::Incoming,
        );
        let page = PageContext {
            project_id,
            heads: &heads,
            generation: &generation,
        };
        if outgoing_offset < outgoing_total {
            let p = current_projection(pin, query.focus, focus_revision, original, limits, &before)
                .await?;
            for (ordinal, p) in p
                .edges
                .iter()
                .enumerate()
                .skip(outgoing_offset)
                .take(query.page_size.get())
            {
                before()?;
                outgoing
                    .results
                    .push(page_edge(query.focus, focus_head, ordinal, p, &page));
            }
        }
        let mut base = 0usize;
        for (source, h) in heads.values() {
            before()?;
            let end = base + h.incoming;
            if h.incoming > 0
                && end > incoming_offset
                && base < incoming_offset.saturating_add(query.page_size.get())
            {
                let p =
                    current_projection(pin, *source, h.revision, original, limits, &before).await?;
                let skip = incoming_offset.saturating_sub(base);
                let remaining = query.page_size.get() - incoming.results.len();
                for (ordinal, p) in p
                    .edges
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| targets(&e.reference, query.focus, project_id))
                    .skip(skip)
                    .take(remaining)
                {
                    before()?;
                    incoming
                        .results
                        .push(page_edge(*source, h, ordinal, p, &page));
                }
            }
            base = end;
        }
        let output = TaskReferencesRead::Published {
            generation,
            focus_revision,
            outgoing: Box::new(outgoing),
            incoming: Box::new(incoming),
            coverage: TaskReferencesCoverage {
                project_id,
                source_kinds: vec![TaskReferenceKind::Task, TaskReferenceKind::Session],
                task_session_incoming_complete: warnings.is_empty(),
                inbound_other_sources_not_qualified: true,
            },
            warnings,
        };

        before()?;
        Ok(output)
    }
}

/// Literal incoming identity; no target lookup or target admission is implied.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReferenceTarget {
    pub target_kind: TaskReferenceKind,
    pub scope: TaskReferenceScope,
    pub id: String,
    pub context: Option<String>,
}
impl TaskReferenceTarget {
    fn matches(&self, reference: &TaskReference) -> bool {
        self.target_kind == reference.target_kind
            && self.scope == reference.scope
            && self.id == reference.id
            && self.context == reference.context
    }
    fn validate(
        &self,
        original: &SnapshotContext,
        project_id: ProjectId,
    ) -> Result<(), TaskStoreError> {
        let project_text;
        let (scope, scope_id) = match &self.scope {
            TaskReferenceScope::Project(id) => {
                project_text = id.to_string();
                ("project", project_text.as_str())
            }
            TaskReferenceScope::User(id) => ("user", id.as_str()),
        };
        let entries = [kernel::AuthoredReferenceEntry::Tuple(
            kernel::ReferenceCandidate {
                target_type: match self.target_kind {
                    TaskReferenceKind::Task => "task",
                    TaskReferenceKind::Session => "session",
                    TaskReferenceKind::Memory => "memory",
                    TaskReferenceKind::Knowledge => "knowledge",
                },
                id: &self.id,
                scope: Some(scope),
                scope_id: Some(scope_id),
                context: self.context.as_deref(),
                relation: "backlink",
                field: None,
            },
        )];
        kernel::project_authored_references(
            original,
            kernel::SourceNamespace {
                scope: kernel::LogicalScope::Project(project_id),
                context: None,
            },
            kernel::AuthoredReferencesField::Sequence(&entries),
        )?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReferenceBacklinksQuery {
    pub target: TaskReferenceTarget,
    pub page_size: NonZeroUsize,
    pub cursor: Option<TaskReferenceBacklinksCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BacklinksFamily {
    TaskSourceBacklinks,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BacklinksBinding {
    corpus_key: String,
    family: BacklinksFamily,
    root: graphit_core::native_path::NativePath,
    project_id: ProjectId,
    project_version: String,
    pin: ReferencePin,
    target: TaskReferenceTarget,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReferenceBacklinksCursor {
    binding: Box<BacklinksBinding>,
    direction: Direction,
    offset: usize,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReferenceBacklinksPage {
    pub results: Vec<TaskReferenceEdge>,
    pub total: usize,
    pub complete: bool,
    pub next_cursor: Option<TaskReferenceBacklinksCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskReferenceBacklinksRead {
    NoCorpus,
    Published {
        generation: TaskGeneration,
        incoming: Box<TaskReferenceBacklinksPage>,
        coverage: TaskReferencesCoverage,
        warnings: Vec<TaskReferencesWarning>,
    },
}

struct BacklinksReadContext<'a> {
    original: &'a SnapshotContext,
    project_id: ProjectId,
    version: &'a str,
    target: &'a TaskReferenceTarget,
    limits: TaskStoreReadLimits,
}

async fn backlinks_heads(
    pin: &graphit_data::lance_branch_store::BranchSnapshot,
    context: &BacklinksReadContext<'_>,
    before: &impl Fn() -> Result<(), TaskStoreError>,
) -> Result<Heads, TaskStoreError> {
    use super::rows;
    use futures::TryStreamExt;
    use graphit_data::lance_module_store::StoreError;
    let BacklinksReadContext {
        original,
        project_id,
        version,
        target,
        limits,
    } = *context;
    let mut heads = Heads::new();
    reference_scan(
        pin,
        "kind IN ('task', 'session')",
        original,
        limits,
        before,
        |row| {
            let (source, h) = head(row, project_id, version)?;
            if heads.insert(source.order(), (source, h)).is_some() {
                return Err(TaskStoreError::Invalid("duplicate reference head"));
            }
            Ok(())
        },
    )
    .await?;
    let mut scan = pin.scan();
    scan.filter("kind = 'reference_projection'")
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    before()?;
    let mut stream = crate::storage::read(original, scan.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    // Only key metadata is retained for old/orphan uniqueness, not
    // historical payloads or a global collection of reference edges.
    let mut keys = std::collections::HashSet::new();
    loop {
        before()?;
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else { break };
        validate_reference_batch(&batch)?;
        for i in 0..batch.num_rows() {
            before()?;
            let rows::Row::ReferenceProjection(p) = rows::decode(&batch, i, original)? else {
                return Err(TaskStoreError::Invalid("reference marker family"));
            };
            if p.project_id != project_id {
                return Err(TaskStoreError::Invalid("foreign reference marker"));
            }
            if !keys.insert(p.key()) {
                return Err(TaskStoreError::Invalid("duplicate reference marker"));
            }
            let Some((_, h)) = heads.get_mut(&p.source.order()) else {
                continue;
            };
            if h.revision != p.revision {
                continue;
            }
            h.observe(&p)?;
            h.incoming = p
                .edges
                .iter()
                .filter(|e| target.matches(&e.reference))
                .count();
        }
    }
    before()?;
    Ok(heads)
}

impl super::NativeTaskStore {
    /// Query Task/Session source backlinks without granting access to the target.
    pub async fn reference_backlinks(
        original: &std::sync::Arc<SnapshotContext>,
        admission: &graphit_auth::ProjectReadAdmission<'_>,
        query: TaskReferenceBacklinksQuery,
        limits: TaskStoreReadLimits,
    ) -> Result<TaskReferenceBacklinksRead, TaskStoreError> {
        use super::native;
        native::check_read(original, admission)?;
        if query.page_size.get() > 100 {
            return Err(TaskStoreError::Invalid("reference page size"));
        }

        native::check_read(original, admission)?;
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        query.target.validate(original, project_id)?;
        let root_path = project.scope.live.owner_root();
        let version = project.version.version();
        if let Some(cursor) = &query.cursor
            && (cursor.direction != Direction::Incoming
                || cursor.binding.family != BacklinksFamily::TaskSourceBacklinks
                || cursor.binding.root.as_path() != root_path
                || cursor.binding.project_id != project_id
                || cursor.binding.project_version != version
                || cursor.binding.target != query.target)
        {
            return Err(TaskStoreError::InvalidCursor);
        }
        native::check_read(original, admission)?;
        let Some(captured) = native::read_pin(original, admission, limits).await? else {
            native::check_read(original, admission)?;
            return if query.cursor.is_some() {
                Err(TaskStoreError::InvalidCursor)
            } else {
                Ok(TaskReferenceBacklinksRead::NoCorpus)
            };
        };
        let before = || {
            native::check_read(original, admission)?;
            captured.guard.revalidate(original)?;
            Ok(())
        };
        before()?;
        let pin = &captured.pin;
        let generation = TaskGeneration {
            version: pin.version(),
            physical_branch: pin.physical_branch().map(str::to_owned),
            inherited_target: pin.inherited_target().map(str::to_owned),
        };

        let binding = BacklinksBinding {
            corpus_key: captured.guard.corpus_key(pin.corpus_uri()),
            family: BacklinksFamily::TaskSourceBacklinks,
            root: graphit_core::native_path::NativePath::new(root_path.to_owned())?,
            project_id,
            project_version: version.to_owned(),
            pin: (&generation).into(),
            target: query.target.clone(),
        };
        let offset = match &query.cursor {
            Some(c) if c.binding.as_ref() == &binding => c.offset,
            Some(_) => return Err(TaskStoreError::InvalidCursor),
            None => 0,
        };
        let context = BacklinksReadContext {
            original,
            project_id,
            version,
            target: &query.target,
            limits,
        };
        let heads = backlinks_heads(pin, &context, &before).await?;
        let mut warnings = Vec::new();
        let mut total = 0usize;
        for (source, h) in heads.values() {
            before()?;
            if !h.marker {
                warnings.push(TaskReferencesWarning::MissingCurrentProjection {
                    source: *source,
                    revision: h.revision,
                });
            }
            total = total
                .checked_add(h.incoming)
                .ok_or(TaskStoreError::Invalid("reference total overflow"))?;
        }
        if offset > total {
            return Err(TaskStoreError::InvalidCursor);
        }
        let end = offset.saturating_add(query.page_size.get());
        let next_cursor = if end < total {
            Some(TaskReferenceBacklinksCursor {
                binding: Box::new(binding.clone()),
                direction: Direction::Incoming,
                offset: end,
            })
        } else {
            None
        };
        let mut incoming = TaskReferenceBacklinksPage {
            results: Vec::new(),
            total,
            complete: warnings.is_empty(),
            next_cursor,
        };
        let page = PageContext {
            project_id,
            heads: &heads,
            generation: &generation,
        };
        let mut base = 0usize;
        for (source, h) in heads.values() {
            before()?;
            let source_end = base
                .checked_add(h.incoming)
                .ok_or(TaskStoreError::Invalid("reference total overflow"))?;
            if h.incoming > 0 && source_end > offset && base < end {
                let p =
                    current_projection(pin, *source, h.revision, original, limits, &before).await?;
                let skip = offset.saturating_sub(base);
                let remaining = query.page_size.get() - incoming.results.len();
                for (ordinal, p) in p
                    .edges
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| query.target.matches(&e.reference))
                    .skip(skip)
                    .take(remaining)
                {
                    before()?;
                    incoming
                        .results
                        .push(page_edge(*source, h, ordinal, p, &page));
                }
            }
            base = source_end;
        }
        let result = TaskReferenceBacklinksRead::Published {
            generation,
            incoming: Box::new(incoming),
            coverage: TaskReferencesCoverage {
                project_id,
                source_kinds: vec![TaskReferenceKind::Task, TaskReferenceKind::Session],
                task_session_incoming_complete: warnings.is_empty(),
                inbound_other_sources_not_qualified: true,
            },
            warnings,
        };

        before()?;
        Ok(result)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::super::{NativeTaskStore, rows, test_fixture::*, write};
    use super::*;
    use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
    use graphit_core::file_lock::Cancellation;
    use std::{num::NonZeroU128, sync::Arc};
    fn limits() -> TaskStoreReadLimits {
        read_limits()
    }
    fn query(session: SessionId) -> TaskReferencesQuery {
        TaskReferencesQuery {
            focus: TaskReferenceRecord::Session(session),
            page_size: NonZeroUsize::new(20).unwrap(),
            outgoing_cursor: None,
            incoming_cursor: None,
        }
    }
    fn backlinks_query() -> TaskReferenceBacklinksQuery {
        TaskReferenceBacklinksQuery {
            target: TaskReferenceTarget {
                target_kind: TaskReferenceKind::Memory,
                scope: TaskReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()),
                id: "opaque".into(),
                context: None,
            },
            page_size: NonZeroUsize::new(20).unwrap(),
            cursor: None,
        }
    }
    fn input() -> CreateSession {
        CreateSession {
            idempotency_key: "session".into(),
            title: "Session".into(),
            description: "Actual native references".into(),
            strategy: "Implement".into(),
        }
    }
    macro_rules! admission {
        ($f:ident,$ctx:ident,$issuer:ident,$authority:ident,$write:ident,$read:ident) => {
            let $issuer = $f.issuer(&$ctx);
            let secret =
                std::fs::read_to_string($f.installation.owner_root().join("daemon/mcp.key"))
                    .unwrap();
            let caller = $issuer.authenticate_runtime(&secret, &$ctx).unwrap();
            let $authority = ProjectRecordAuthority::new(&$issuer);
            let $write = $authority
                .admit_authenticated_write(&caller, &$ctx, $f.scope(), ProjectWriteOperation::Task)
                .unwrap();
            let $read = $authority
                .admit_authenticated_read(&caller, &$ctx, $f.scope(), ProjectReadModule::Task)
                .unwrap();
        };
    }
    async fn publish(
        original: &Arc<SnapshotContext>,
        admission: &graphit_auth::ProjectWriteAdmission<'_>,
        row: rows::Row,
    ) {
        let captured = write::acquire(original, admission, write_limits(), false)
            .await
            .unwrap();
        write::commit(captured, original, admission, vec![row], None)
            .await
            .unwrap();
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn backlinks_actual_sdk_rejects_duplicate_old_marker_after_current_head_validation() {
        use futures::TryStreamExt;
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        admission!(f, ctx, issuer, authority, write, read);
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            input(),
            kernel::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let secret =
            std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
        let coordinate = authority
            .admit_authenticated_write(
                &caller,
                &ctx,
                f.scope(),
                ProjectWriteOperation::CoordinateTask,
            )
            .unwrap();
        NativeTaskStore::claim_session(
            ctx.snapshot(),
            &coordinate,
            session.id,
            std::time::Duration::from_secs(120),
            write_limits(),
        )
        .await
        .unwrap();
        let captured = super::super::native::read_pin(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
            .unwrap();
        let mut stream = captured.pin.scan().try_into_stream().await.unwrap();
        let mut batches = Vec::new();
        while let Some(batch) = stream.try_next().await.unwrap() {
            batches.push(batch);
        }
        drop(stream);
        drop(captured);
        let duplicate = session_projection(
            &session,
            "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            "main",
        );
        batches.push(rows::batch(&[rows::Row::ReferenceProjection(duplicate)]).unwrap());
        let uri = super::super::native::uri(&f.project, std::path::Path::new("task/index.lance"))
            .unwrap();
        let store = graphit_data::lance_module_store::LanceModuleStore::open(&uri, rows::schema())
            .await
            .unwrap();
        store
            .snapshot()
            .prepare_replace(batches)
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
        let mut bounded = limits();
        bounded.batch_rows = std::num::NonZeroU32::new(1).unwrap();
        assert!(matches!(
            NativeTaskStore::reference_backlinks(ctx.snapshot(), &read, backlinks_query(), bounded)
                .await,
            Err(TaskStoreError::Invalid("duplicate reference marker"))
        ));
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn references_missing_current_marker_is_incomplete_and_never_uses_old_generation() {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        admission!(f, ctx, issuer, authority, write, read);
        let mut session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            input(),
            kernel::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        session.revision += 1;
        session.reference_generation = session.revision;
        // A fixture-only canonical head without its next marker models lost input.
        publish(
            ctx.snapshot(),
            &write,
            rows::Row::Session(rows::Stored {
                record: session.clone(),
                fence: None,
            }),
        )
        .await;
        let TaskReferencesRead::Published {
            outgoing,
            incoming,
            warnings,
            coverage,
            ..
        } = NativeTaskStore::references(ctx.snapshot(), &read, query(session.id), limits())
            .await
            .unwrap()
        else {
            panic!("published")
        };
        assert!(outgoing.results.is_empty() && !outgoing.complete && !incoming.complete);
        assert!(!coverage.task_session_incoming_complete);
        assert_eq!(
            warnings,
            vec![TaskReferencesWarning::MissingCurrentProjection {
                source: TaskReferenceRecord::Session(session.id),
                revision: 2
            }]
        );
        let TaskReferenceBacklinksRead::Published {
            incoming,
            coverage,
            warnings,
            ..
        } = NativeTaskStore::reference_backlinks(
            ctx.snapshot(),
            &read,
            backlinks_query(),
            limits(),
        )
        .await
        .unwrap()
        else {
            panic!("published incomplete backlinks")
        };
        assert!(!incoming.complete && incoming.results.is_empty());
        assert!(!coverage.task_session_incoming_complete && warnings.len() == 1);
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn references_current_marker_compares_actual_authored_state_not_only_revision() {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        admission!(f, ctx, issuer, authority, write, read);
        let entries = [kernel::AuthoredReferenceEntry::Tuple(
            kernel::ReferenceCandidate {
                target_type: "memory",
                id: "id",
                relation: "supports",
                scope: None,
                scope_id: None,
                context: None,
                field: None,
            },
        )];
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            input(),
            kernel::AuthoredReferencesField::Sequence(&entries),
            write_limits(),
        )
        .await
        .unwrap();
        let project = write.project().unwrap();
        let pid = super::super::native::project_id(project.scope.live).unwrap();
        let mut marker = session_projection(&session, pid, project.version.version());
        marker.edges[0].reference.relation = "different".into();
        publish(
            ctx.snapshot(),
            &write,
            rows::Row::ReferenceProjection(marker),
        )
        .await;
        assert!(matches!(
            NativeTaskStore::references(ctx.snapshot(), &read, query(session.id), limits()).await,
            Err(TaskStoreError::Invalid(
                "current reference marker disagrees with head"
            ))
        ));
        assert!(matches!(
            NativeTaskStore::reference_backlinks(
                ctx.snapshot(),
                &read,
                backlinks_query(),
                limits()
            )
            .await,
            Err(TaskStoreError::Invalid(
                "current reference marker disagrees with head"
            ))
        ));
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn references_foreign_project_is_rejected_and_publication_label_is_not_a_grant() {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        admission!(f, ctx, issuer, authority, write, read);
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            input(),
            kernel::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let project = write.project().unwrap();
        let pid = super::super::native::project_id(project.scope.live).unwrap();
        let foreign_pid: ProjectId = "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap();
        assert_ne!(pid, foreign_pid);
        let mut marker = session_projection(&session, foreign_pid, project.version.version());
        publish(
            ctx.snapshot(),
            &write,
            rows::Row::ReferenceProjection(marker.clone()),
        )
        .await;
        assert!(matches!(
            NativeTaskStore::references(ctx.snapshot(), &read, query(session.id), limits()).await,
            Err(TaskStoreError::Invalid("foreign reference marker"))
        ));
        marker.project_id = pid;
        marker.project_version = "feature/foreign".into();
        publish(
            ctx.snapshot(),
            &write,
            rows::Row::ReferenceProjection(marker),
        )
        .await;
        assert!(matches!(
            NativeTaskStore::references(ctx.snapshot(), &read, query(session.id), limits()).await,
            Ok(TaskReferencesRead::Published { .. })
        ));
        assert!(matches!(
            NativeTaskStore::reference_backlinks(
                ctx.snapshot(),
                &read,
                backlinks_query(),
                limits()
            )
            .await,
            Ok(TaskReferenceBacklinksRead::Published { .. })
        ));
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn references_actual_sdk_rejects_duplicate_marker_patch_without_partial_publication() {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        admission!(f, ctx, issuer, authority, write, read);
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            input(),
            kernel::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let before =
            NativeTaskStore::references(ctx.snapshot(), &read, query(session.id), limits())
                .await
                .unwrap();
        let project = write.project().unwrap();
        let pid = super::super::native::project_id(project.scope.live).unwrap();
        let marker = session_projection(&session, pid, project.version.version());
        let batch = rows::batch(&[
            rows::Row::ReferenceProjection(marker.clone()),
            rows::Row::ReferenceProjection(marker.clone()),
        ])
        .unwrap();
        let capture = write::acquire(ctx.snapshot(), &write, write_limits(), false)
            .await
            .unwrap();
        let result = capture
            .pin
            .prepare_merge(vec![batch], vec!["row_key".into()])
            .await;
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("actual SDK must reject duplicate source keys"),
        };
        let message = error.to_string().to_lowercase();
        assert!(
            message.contains("duplicate") || message.contains("multiple"),
            "{message}"
        );
        drop(capture);
        let after = NativeTaskStore::references(ctx.snapshot(), &read, query(session.id), limits())
            .await
            .unwrap();
        assert_eq!(before, after);
        // A second payload can only get another SDK key; its noncanonical key
        // must then fail the full read rather than duplicate current edges.
        let batch = rows::batch(&[rows::Row::ReferenceProjection(marker)]).unwrap();
        let mut columns = batch.columns().to_vec();
        let key_index = batch.schema().index_of("row_key").unwrap();
        let keys = batch
            .column(key_index)
            .as_any()
            .downcast_ref::<arrow_array::StringArray>()
            .unwrap();
        columns[key_index] = Arc::new(arrow_array::StringArray::from(vec![format!(
            "{}/duplicate",
            keys.value(0)
        )]));
        let duplicate = arrow_array::RecordBatch::try_new(batch.schema(), columns).unwrap();
        let capture = write::acquire(ctx.snapshot(), &write, write_limits(), false)
            .await
            .unwrap();
        capture
            .pin
            .prepare_merge(vec![duplicate], vec!["row_key".into()])
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
        drop(capture);
        assert!(
            NativeTaskStore::references(ctx.snapshot(), &read, query(session.id), limits())
                .await
                .is_err()
        );
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn references_valid_orphan_is_not_adopted_but_consulted_invalid_orphan_fails_full_eof() {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        admission!(f, ctx, issuer, authority, write, read);
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            input(),
            kernel::AuthoredReferencesField::Sequence(&[]),
            write_limits(),
        )
        .await
        .unwrap();
        let p = write.project().unwrap();
        let pid = super::super::native::project_id(p.scope.live).unwrap();
        let mut marker = session_projection(&session, pid, p.version.version());
        marker.source = TaskReferenceRecord::Session(SessionId::new(NonZeroU128::new(99).unwrap()));
        marker.revision = 9;
        marker.reference_generation = 9;
        publish(
            ctx.snapshot(),
            &write,
            rows::Row::ReferenceProjection(marker.clone()),
        )
        .await;
        let TaskReferencesRead::Published {
            outgoing, warnings, ..
        } = NativeTaskStore::references(ctx.snapshot(), &read, query(session.id), limits())
            .await
            .unwrap()
        else {
            panic!("published")
        };
        assert!(outgoing.complete && outgoing.results.is_empty() && warnings.is_empty());
        marker.edges.push(ProjectedReference {
            provenance: TaskReferenceProvenance::Authored,
            reference: TaskReference {
                target_kind: TaskReferenceKind::Memory,
                scope: TaskReferenceScope::Project(pid),
                id: "id".into(),
                context: None,
                relation: String::new(),
                field: None,
            },
        });
        publish(
            ctx.snapshot(),
            &write,
            rows::Row::ReferenceProjection(marker),
        )
        .await;
        assert!(
            NativeTaskStore::references(ctx.snapshot(), &read, query(session.id), limits())
                .await
                .is_err()
        );
        // Existing FTS preflight must decode non-current payloads too.
        assert!(
            NativeTaskStore::create_lexical_index(ctx.snapshot(), &write, write_limits())
                .await
                .is_err()
        );
        assert!(
            NativeTaskStore::reference_backlinks(
                ctx.snapshot(),
                &read,
                backlinks_query(),
                limits()
            )
            .await
            .is_err()
        );
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn references_header_and_public_search_are_canonical_even_for_old_markers() {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        admission!(f, ctx, issuer, authority, write, read);
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            input(),
            kernel::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let p = write.project().unwrap();
        let pid = super::super::native::project_id(p.scope.live).unwrap();
        let mut marker = session_projection(&session, pid, p.version.version());
        marker.revision = 8;
        marker.reference_generation = 8;
        let batch = rows::batch(&[rows::Row::ReferenceProjection(marker)]).unwrap();
        let mut columns = batch.columns().to_vec();
        let index = batch.schema().index_of("public_search").unwrap();
        columns[index] = Arc::new(arrow_array::StringArray::from(vec![
            "{\"private\":\"forbidden\"}",
        ]));
        let tampered = arrow_array::RecordBatch::try_new(batch.schema(), columns).unwrap();
        let capture = write::acquire(ctx.snapshot(), &write, write_limits(), false)
            .await
            .unwrap();
        capture
            .pin
            .prepare_merge(vec![tampered], vec!["row_key".into()])
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
        drop(capture);
        assert!(
            NativeTaskStore::references(ctx.snapshot(), &read, query(session.id), limits())
                .await
                .is_err()
        );
        assert!(
            NativeTaskStore::reference_backlinks(
                ctx.snapshot(),
                &read,
                backlinks_query(),
                limits()
            )
            .await
            .is_err()
        );
    }
    #[test]
    fn references_snapshots_require_fields_and_generation_matches_domain_revision() {
        let session = Session {
            id: SessionId::new(NonZeroU128::new(1).unwrap()),
            input: input(),
            revision: 1,
            references: vec![],
            reference_generation: 1,
            status: crate::gates::TaskStatus::Open,
            ownership: Ownership::default(),
            progress: None,
            created_at: 1,
            updated_at: 1,
        };
        let mut value = serde_json::to_value(&session).unwrap();
        value.as_object_mut().unwrap().remove("references");
        assert!(serde_json::from_value::<Session>(value).is_err());
        let mut value = serde_json::to_value(&session).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("reference_generation");
        assert!(serde_json::from_value::<Session>(value).is_err());
        let ctx = SnapshotContext::new(
            std::time::Instant::now() + std::time::Duration::from_secs(10),
            Cancellation::default(),
        );
        let mut wrong = session;
        wrong.reference_generation = 2;
        let batch = rows::batch(&[rows::Row::Session(rows::Stored {
            record: wrong,
            fence: None,
        })])
        .unwrap();
        assert!(rows::decode(&batch, 0, &ctx).is_err());
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn references_actual_read_admission_revocation_and_cancellation_rechecked_mid_scan() {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        let issuer = f.issuer(&ctx);
        let (_, reference, _) = f.issue(&issuer, &ctx);
        let secret =
            std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
        let admin = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &admin).unwrap();
        let run = issuer
            .start_run(&parent, &f.scope(), std::time::Duration::from_secs(120))
            .unwrap();
        let permissions = [
            graphit_auth::Permission::Read(graphit_core::module_settings::Module::Task),
            graphit_auth::Permission::WriteTask,
        ];
        let bearer = issuer
            .issue(&parent, &run, &reference, &permissions, &f.scope())
            .unwrap();
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let write = authority
            .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
            .unwrap();
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Task)
            .unwrap();
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            input(),
            kernel::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let captured = super::super::native::read_pin(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
            .unwrap();
        let before = || {
            super::super::native::check_read(ctx.snapshot(), &read)?;
            captured.guard.revalidate(ctx.snapshot())?;
            Ok(())
        };
        let error = reference_scan(
            &captured.pin,
            "kind = 'session'",
            ctx.snapshot(),
            limits(),
            &before,
            |_| {
                issuer.revoke(&parent, &bearer).unwrap();
                Ok(())
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(error, TaskStoreError::Auth(_)));
        assert!(
            NativeTaskStore::references(ctx.snapshot(), &read, query(session.id), limits())
                .await
                .is_err()
        );
        assert!(matches!(
            NativeTaskStore::reference_backlinks(
                ctx.snapshot(),
                &read,
                backlinks_query(),
                limits()
            )
            .await,
            Err(TaskStoreError::Auth(_))
        ));
        drop(captured);
        let bearer = issuer
            .issue(&parent, &run, &reference, &permissions, &f.scope())
            .unwrap();
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Task)
            .unwrap();
        let captured = super::super::native::read_pin(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
            .unwrap();
        let before = || {
            super::super::native::check_read(ctx.snapshot(), &read)?;
            captured.guard.revalidate(ctx.snapshot())?;
            Ok(())
        };
        let mut observed = false;
        let error = reference_scan(
            &captured.pin,
            "kind = 'session'",
            ctx.snapshot(),
            limits(),
            &before,
            |_| {
                observed = true;
                cancel.cancel();
                Ok(())
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            TaskStoreError::Request(_) | TaskStoreError::Auth(_)
        ));
        assert!(observed);
    }
}
