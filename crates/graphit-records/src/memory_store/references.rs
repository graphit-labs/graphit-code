use super::{Memory, MemoryId, MemoryReference, MemoryStoreError, MemoryStoreWriteLimits, rows};
use super::{
    MemoryGeneration, MemoryReferenceScope, MemoryStoreReadLimits, NativeMemoryStore, native,
};
use crate::reference_projection::ReferenceTargetKind;
use arrow_array::RecordBatch;
use graphit_auth::ProjectReadAdmission;
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::StoreError};
use serde::{Deserialize, Serialize};
use std::io;
use std::{
    collections::{BTreeMap, HashSet},
    num::NonZeroUsize,
    sync::Arc,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReferenceTarget {
    #[serde(with = "super::model::kind_codec")]
    pub target_kind: ReferenceTargetKind,
    pub scope: MemoryReferenceScope,
    pub id: String,
    pub context: Option<String>,
}
impl MemoryReferenceTarget {
    fn matches(&self, r: &MemoryReference) -> bool {
        self.target_kind == r.target_kind
            && self.scope == r.scope
            && self.id == r.id
            && self.context == r.context
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReferencesQuery {
    pub outgoing: Option<MemoryId>,
    pub incoming: Option<MemoryReferenceTarget>,
    pub page_size: NonZeroUsize,
    pub outgoing_cursor: Option<MemoryReferencesCursor>,
    pub incoming_cursor: Option<MemoryReferencesCursor>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Direction {
    Outgoing,
    Incoming,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    corpus_key: String,
    root: graphit_core::native_path::NativePath,
    generation: MemoryGeneration,
    outgoing: Option<MemoryId>,
    incoming: Option<MemoryReferenceTarget>,
}
/// Closed observation data. A continuation always needs a fresh live admission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReferencesCursor {
    binding: Box<Binding>,
    direction: Direction,
    offset: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryReferenceProvenance {
    Authored,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MemoryReferenceResolution {
    Memory { id: MemoryId, title: String },
    Unresolved,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReferenceEdge {
    pub source: MemoryId,
    pub source_revision: u64,
    pub reference_generation: u64,
    pub source_snapshot: MemoryGeneration,
    pub reference: MemoryReference,
    pub provenance: MemoryReferenceProvenance,
    pub ordinal: usize,
    pub source_observation: MemoryReferenceResolution,
    pub target_resolution: MemoryReferenceResolution,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReferencePage {
    pub results: Vec<MemoryReferenceEdge>,
    pub total: usize,
    pub complete: bool,
    pub next_cursor: Option<MemoryReferencesCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReferencesCoverage {
    pub project_id: ProjectId,
    pub memory_project_source_complete: bool,
    pub inbound_other_sources_not_qualified: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MemoryReferencesWarning {
    MissingCurrentProjection { source: MemoryId, revision: u64 },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MemoryReferencesRead {
    NoCorpus,
    Published {
        generation: MemoryGeneration,
        outgoing: Option<Box<MemoryReferencePage>>,
        incoming: Option<Box<MemoryReferencePage>>,
        coverage: MemoryReferencesCoverage,
        warnings: Vec<MemoryReferencesWarning>,
    },
}

pub(crate) struct Head {
    pub revision: u64,
    pub reference_generation: u64,
    pub tombstone: bool,
    pub title: String,
    operation_version: Option<String>,
    marker: bool,
    outgoing: usize,
    incoming: usize,
}
pub(crate) type Heads = BTreeMap<MemoryId, Head>;

#[derive(Clone, Copy)]
pub(crate) enum ReadOwner<'a> {
    Project(ProjectId),
    User(&'a crate::user_memory_store::UserMemoryOwner),
}
impl ReadOwner<'_> {
    fn schema(self) -> Arc<arrow_schema::Schema> {
        match self {
            Self::Project(_) => rows::schema(),
            Self::User(_) => crate::user_memory_store::user_memory_schema(),
        }
    }
    fn head(
        self,
        batch: &RecordBatch,
        row: usize,
        original: &SnapshotContext,
    ) -> Result<ReadHead, MemoryStoreError> {
        let decoded = match self {
            Self::Project(project_id) => {
                let row = rows::decode(batch, row, original, project_id)?
                    .ok_or(MemoryStoreError::Invalid("Memory reference head family"))?;
                if row.kind != "head" {
                    return Err(MemoryStoreError::Invalid("Memory reference head family"));
                }
                ReadHead {
                    memory: row.memory,
                    tombstone: row.tombstone,
                    operation_version: Some(row.operation_version),
                }
            }
            Self::User(owner) => {
                let row = crate::user_memory_store::rows::decode(batch, row, original, owner)?
                    .ok_or(MemoryStoreError::Invalid("USER reference head family"))?;
                if row.kind != "head" {
                    return Err(MemoryStoreError::Invalid("USER reference head family"));
                }
                ReadHead {
                    memory: row.memory,
                    tombstone: row.tombstone,
                    operation_version: None,
                }
            }
        };
        Ok(decoded)
    }
    fn marker(
        self,
        batch: &RecordBatch,
        row: usize,
        original: &SnapshotContext,
    ) -> Result<ReferenceProjection, MemoryStoreError> {
        match self {
            Self::Project(project_id) => {
                validate_embedding_nulls(batch, row)?;
                let p = decode(batch, row, original, project_id)?;
                Ok(ReferenceProjection {
                    source: p.source,
                    operation_version: Some(p.operation_version),
                    revision: p.revision,
                    reference_generation: p.reference_generation,
                    tombstone: p.tombstone,
                    references: p.references,
                })
            }
            Self::User(owner) => {
                let p = crate::user_memory_store::rows::decode_reference_marker(
                    batch, row, original, owner,
                )?;
                Ok(ReferenceProjection {
                    source: p.source,
                    operation_version: None,
                    revision: p.revision,
                    reference_generation: p.reference_generation,
                    tombstone: p.tombstone,
                    references: p.references,
                })
            }
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct ReadIdentity<'a> {
    pub original: &'a SnapshotContext,
    pub owner: ReadOwner<'a>,
}
struct ReadHead {
    memory: Memory,
    tombstone: bool,
    operation_version: Option<String>,
}
struct ReferenceProjection {
    source: MemoryId,
    operation_version: Option<String>,
    revision: u64,
    reference_generation: u64,
    tombstone: bool,
    references: Vec<MemoryReference>,
}

async fn head_scan(
    pin: &BranchSnapshot,
    filter: &str,
    identity: ReadIdentity<'_>,
    limits: MemoryStoreReadLimits,
    before: &impl Fn() -> Result<(), MemoryStoreError>,
    mut each: impl FnMut(ReadHead) -> Result<(), MemoryStoreError>,
) -> Result<(), MemoryStoreError> {
    let ReadIdentity { original, owner } = identity;
    use futures::TryStreamExt;
    before()?;
    let mut scanner = pin.scan();
    scanner
        .filter(filter)
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    let mut stream = crate::storage::read(original, scanner.try_into_stream())
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
        rows::validate_schema(&batch, owner.schema().as_ref())?;
        for row in 0..batch.num_rows() {
            before()?;
            let decoded = owner.head(&batch, row, original)?;
            each(decoded)?;
        }
    }
    before()?;
    Ok(())
}

fn head_filter(id: MemoryId) -> String {
    format!(
        "kind = 'head' AND record_id = {}",
        native::literal(&id.to_string())
    )
}

async fn current_projection(
    pin: &BranchSnapshot,
    id: MemoryId,
    revision: u64,
    identity: ReadIdentity<'_>,
    limits: MemoryStoreReadLimits,
    before: &impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<ReferenceProjection, MemoryStoreError> {
    let ReadIdentity { original, owner } = identity;
    use futures::TryStreamExt;
    before()?;
    let mut scanner = pin.scan();
    scanner
        .filter(&format!(
            "kind = 'references' AND row_key = {}",
            native::literal(&format!("references/{id}/{revision}"))
        ))
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    let mut stream = crate::storage::read(original, scanner.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut found = None;
    loop {
        before()?;
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else { break };
        rows::validate_schema(&batch, owner.schema().as_ref())?;
        for row in 0..batch.num_rows() {
            before()?;
            let p = owner.marker(&batch, row, original)?;
            if p.source != id || p.revision != revision || found.replace(p).is_some() {
                return Err(MemoryStoreError::Invalid(
                    "duplicate Memory reference marker",
                ));
            }
        }
    }
    before()?;
    found.ok_or(MemoryStoreError::Invalid(
        "validated Memory reference marker vanished",
    ))
}

fn validate_embedding_nulls(batch: &RecordBatch, row: usize) -> Result<(), MemoryStoreError> {
    if batch.columns().iter().skip(18).any(|c| !c.is_null(row)) {
        return Err(MemoryStoreError::Invalid(
            "Memory reference embedding fields",
        ));
    }
    Ok(())
}

fn resolution<'a>(
    r: &MemoryReference,
    project_id: ProjectId,
    heads: &'a Heads,
) -> Option<(MemoryId, &'a str)> {
    if r.target_kind != ReferenceTargetKind::Memory
        || r.scope != MemoryReferenceScope::Project(project_id)
        || r.context.is_some()
    {
        return None;
    }
    let id: MemoryId = r.id.parse().ok()?;
    let h = heads.get(&id).filter(|h| !h.tombstone)?;
    Some((id, &h.title))
}

struct PageContext<'a> {
    generation: &'a MemoryGeneration,
    heads: &'a Heads,
    original: &'a SnapshotContext,
}
fn page_edge(
    source: MemoryId,
    h: &Head,
    ordinal: usize,
    r: &MemoryReference,
    page: &PageContext<'_>,
) -> Result<MemoryReferenceEdge, MemoryStoreError> {
    let target = resolution(r, page.generation.project_id, page.heads);

    page.original.check()?;
    Ok(MemoryReferenceEdge {
        source,
        source_revision: h.revision,
        reference_generation: h.reference_generation,
        source_snapshot: page.generation.clone(),
        reference: r.clone(),
        provenance: MemoryReferenceProvenance::Authored,
        ordinal,
        source_observation: MemoryReferenceResolution::Memory {
            id: source,
            title: h.title.clone(),
        },
        target_resolution: target.map_or(MemoryReferenceResolution::Unresolved, |(id, title)| {
            MemoryReferenceResolution::Memory {
                id,
                title: title.to_owned(),
            }
        }),
    })
}

pub(crate) struct PinnedQuery<'a> {
    pub identity: ReadIdentity<'a>,
    pub outgoing: Option<MemoryId>,
    pub incoming: Option<&'a MemoryReferenceTarget>,
    pub page_size: NonZeroUsize,
    pub outgoing_offset: usize,
    pub incoming_offset: usize,
    pub limits: MemoryStoreReadLimits,
}
pub(crate) struct QueriedPage<E, C> {
    pub results: Vec<E>,
    pub total: usize,
    pub complete: bool,
    pub next_cursor: Option<C>,
}
pub(crate) struct QueriedPages<E, C> {
    pub outgoing: Option<QueriedPage<E, C>>,
    pub incoming: Option<QueriedPage<E, C>>,
    pub warnings: Vec<MemoryReferencesWarning>,
}
/// One pin/EOF query flow. Only the two typed facades select namespace codecs and output data.
pub(crate) async fn query_pin<E, C>(
    pin: &BranchSnapshot,
    query: PinnedQuery<'_>,
    before: &impl Fn() -> Result<(), MemoryStoreError>,
    mut edge: impl FnMut(
        MemoryId,
        &Head,
        usize,
        &MemoryReference,
        &Heads,
    ) -> Result<E, MemoryStoreError>,
    mut cursor: impl FnMut(Direction, usize) -> Result<C, MemoryStoreError>,
) -> Result<QueriedPages<E, C>, MemoryStoreError> {
    let identity = query.identity;
    let original = identity.original;
    let limits = query.limits;
    let outgoing_offset = query.outgoing_offset;
    let incoming_offset = query.incoming_offset;
    let mut heads = Heads::new();
    head_scan(
        pin,
        "kind = 'head' OR row_key LIKE 'head/%'",
        identity,
        limits,
        &before,
        |row| {
            let h = Head {
                revision: row.memory.revision,
                reference_generation: row.memory.reference_generation,
                tombstone: row.tombstone,
                title: row.memory.title,
                operation_version: row.operation_version,
                marker: false,
                outgoing: 0,
                incoming: 0,
            };
            if heads.insert(row.memory.id, h).is_some() {
                return Err(MemoryStoreError::Invalid("duplicate Memory reference head"));
            }
            Ok(())
        },
    )
    .await?;
    if query
        .outgoing
        .is_some_and(|id| heads.get(&id).is_none_or(|h| h.tombstone))
    {
        return Err(MemoryStoreError::NotFound);
    }
    // Every marker is canonical, including old and orphan rows. Current rows
    // additionally compare to the actual same-pin head; bodies are discarded.
    use futures::TryStreamExt;
    let mut scanner = pin.scan();
    scanner
        .filter("kind = 'references' OR row_key LIKE 'references/%'")
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    before()?;
    let mut stream = crate::storage::read(original, scanner.try_into_stream())
        .await?
        .map_err(StoreError::from)?;
    before()?;
    let mut keys = HashSet::new();
    loop {
        before()?;
        let next = crate::storage::read(original, stream.try_next())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else { break };
        rows::validate_schema(&batch, identity.owner.schema().as_ref())?;
        for row in 0..batch.num_rows() {
            before()?;
            if !keys.insert(rows::text(&batch, 0, row)?.to_owned()) {
                return Err(MemoryStoreError::Invalid(
                    "duplicate Memory reference marker",
                ));
            }
            let p = identity.owner.marker(&batch, row, original)?;
            let Some(h) = heads.get_mut(&p.source) else {
                continue;
            };
            if p.revision != h.revision {
                continue;
            }
            if h.marker {
                return Err(MemoryStoreError::Invalid(
                    "duplicate current Memory reference marker",
                ));
            }
            let mut matched = false;
            head_scan(
                pin,
                &head_filter(p.source),
                identity,
                limits,
                &before,
                |row| {
                    if matched
                        || row.memory.id != p.source
                        || row.memory.revision != p.revision
                        || row.memory.reference_generation != p.reference_generation
                        || row.operation_version != p.operation_version
                        || row.tombstone != p.tombstone
                        || row.memory.references != p.references
                    {
                        return Err(MemoryStoreError::Invalid(
                            "Memory reference marker disagrees with head",
                        ));
                    }
                    matched = true;
                    Ok(())
                },
            )
            .await?;
            if !matched {
                return Err(MemoryStoreError::Invalid("Memory reference head vanished"));
            }
            if h.operation_version != p.operation_version
                || h.reference_generation != p.reference_generation
                || h.tombstone != p.tombstone
            {
                return Err(MemoryStoreError::Invalid("Memory reference marker binding"));
            }
            h.marker = true;
            if !h.tombstone {
                h.outgoing = p.references.len();
                h.incoming = query
                    .incoming
                    .map_or(0, |t| p.references.iter().filter(|r| t.matches(r)).count());
            }
        }
    }
    before()?;
    let mut warnings = Vec::new();
    let mut incoming_total = 0usize;
    for (id, h) in &heads {
        before()?;
        if !h.marker {
            warnings.push(MemoryReferencesWarning::MissingCurrentProjection {
                source: *id,
                revision: h.revision,
            });
        }
        incoming_total = incoming_total
            .checked_add(h.incoming)
            .ok_or(MemoryStoreError::Invalid("Memory reference total overflow"))?;
    }
    let outgoing_total = query
        .outgoing
        .and_then(|id| heads.get(&id))
        .map_or(0, |h| h.outgoing);
    if outgoing_offset > outgoing_total || incoming_offset > incoming_total {
        return Err(MemoryStoreError::Invalid("Memory reference cursor"));
    }
    let mut make_page = |total: usize,
                         complete: bool,
                         offset: usize,
                         direction: Direction|
     -> Result<QueriedPage<E, C>, MemoryStoreError> {
        let next_offset =
            offset
                .checked_add(query.page_size.get())
                .ok_or(MemoryStoreError::Invalid(
                    "Memory reference offset overflow",
                ))?;
        let next_cursor = if next_offset < total {
            Some(cursor(direction, next_offset)?)
        } else {
            None
        };
        Ok(QueriedPage {
            results: Vec::new(),
            total,
            complete,
            next_cursor,
        })
    };
    let mut outgoing = match query.outgoing {
        Some(id) => Some(make_page(
            outgoing_total,
            heads[&id].marker,
            outgoing_offset,
            Direction::Outgoing,
        )?),
        None => None,
    };
    let mut incoming = match query.incoming {
        Some(_) => Some(make_page(
            incoming_total,
            warnings.is_empty(),
            incoming_offset,
            Direction::Incoming,
        )?),
        None => None,
    };
    if let (Some(id), Some(page)) = (query.outgoing, &mut outgoing)
        && outgoing_offset < outgoing_total
    {
        let h = &heads[&id];
        let p = current_projection(pin, id, h.revision, identity, limits, &before).await?;
        for (ordinal, r) in p
            .references
            .iter()
            .enumerate()
            .skip(outgoing_offset)
            .take(query.page_size.get())
        {
            before()?;
            page.results.push(edge(id, h, ordinal, r, &heads)?);
        }
    }
    if let (Some(target), Some(page)) = (query.incoming, &mut incoming) {
        let mut base = 0usize;
        let page_end =
            incoming_offset
                .checked_add(query.page_size.get())
                .ok_or(MemoryStoreError::Invalid(
                    "Memory reference offset overflow",
                ))?;
        for (id, h) in &heads {
            before()?;
            let end = base
                .checked_add(h.incoming)
                .ok_or(MemoryStoreError::Invalid("Memory reference total overflow"))?;
            if h.incoming > 0 && end > incoming_offset && base < page_end {
                let p = current_projection(pin, *id, h.revision, identity, limits, &before).await?;
                let remaining = query.page_size.get() - page.results.len();
                for (ordinal, r) in p
                    .references
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| target.matches(r))
                    .skip(incoming_offset.saturating_sub(base))
                    .take(remaining)
                {
                    before()?;
                    page.results.push(edge(*id, h, ordinal, r, &heads)?);
                }
            }
            base = end;
        }
    }
    before()?;
    Ok(QueriedPages {
        outgoing,
        incoming,
        warnings,
    })
}

impl NativeMemoryStore {
    /// Read only the authorized PROJECT Memory source from one observed SDK pin.
    pub async fn references(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        query: MemoryReferencesQuery,
        limits: MemoryStoreReadLimits,
    ) -> Result<MemoryReferencesRead, MemoryStoreError> {
        native::check_read(original, admission)?;
        if query.page_size.get() > 100
            || query.outgoing.is_none() && query.incoming.is_none()
            || query.outgoing.is_none() && query.outgoing_cursor.is_some()
            || query.incoming.is_none() && query.incoming_cursor.is_some()
        {
            return Err(MemoryStoreError::Invalid("Memory reference query"));
        }
        original.check()?;
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let identity = ReadIdentity {
            original,
            owner: ReadOwner::Project(project_id),
        };
        let root =
            graphit_core::native_path::NativePath::new(project.scope.live.owner_root().to_owned())?;
        if let Some(target) = &query.incoming {
            use crate::reference_projection::{
                AuthoredReferenceEntry, AuthoredReferencesField, LogicalScope, ReferenceCandidate,
                SourceNamespace, project_authored_references,
            };
            let scope_id = match &target.scope {
                MemoryReferenceScope::Project(p) => p.to_string(),
                MemoryReferenceScope::User(u) => u.clone(),
            };
            let candidates = [AuthoredReferenceEntry::Tuple(ReferenceCandidate {
                target_type: super::model::kind(target.target_kind),
                id: &target.id,
                scope: Some(match target.scope {
                    MemoryReferenceScope::Project(_) => "project",
                    MemoryReferenceScope::User(_) => "user",
                }),
                scope_id: Some(&scope_id),
                context: target.context.as_deref(),
                relation: "backlink",
                field: None,
            })];
            project_authored_references(
                original,
                SourceNamespace {
                    scope: LogicalScope::Project(project_id),
                    context: None,
                },
                AuthoredReferencesField::Sequence(&candidates),
            )?;
        }
        for (cursor, direction) in [
            (&query.outgoing_cursor, Direction::Outgoing),
            (&query.incoming_cursor, Direction::Incoming),
        ] {
            if let Some(c) = cursor
                && (c.direction != direction
                    || c.binding.outgoing != query.outgoing
                    || c.binding.incoming != query.incoming
                    || c.binding.root.as_path() != project.scope.live.owner_root()
                    || c.binding.generation.project_id != project_id
                    || c.binding.generation.project_version != project.version.version())
            {
                return Err(MemoryStoreError::Invalid("Memory reference cursor"));
            }
        }
        native::check_read(original, admission)?;
        let Some(capture) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return if query.outgoing_cursor.is_some() || query.incoming_cursor.is_some() {
                Err(MemoryStoreError::Invalid("Memory reference cursor"))
            } else {
                Ok(MemoryReferencesRead::NoCorpus)
            };
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok(())
        };
        before()?;
        let pin = &capture.pin;
        let generation = native::generation(pin, project_id, project.version.version());

        original.check()?;
        let binding = Binding {
            corpus_key: capture.guard.corpus_key(pin.corpus_uri()),
            root,
            generation: generation.clone(),
            outgoing: query.outgoing,
            incoming: query.incoming.clone(),
        };
        let offset = |cursor: &Option<MemoryReferencesCursor>, direction| match cursor {
            None => Ok(0),
            Some(c) if c.binding.as_ref() == &binding && c.direction == direction => Ok(c.offset),
            _ => Err(MemoryStoreError::Invalid("Memory reference cursor")),
        };
        let outgoing_offset = offset(&query.outgoing_cursor, Direction::Outgoing)?;
        let incoming_offset = offset(&query.incoming_cursor, Direction::Incoming)?;
        let result = query_pin(
            pin,
            PinnedQuery {
                identity,
                outgoing: query.outgoing,
                incoming: query.incoming.as_ref(),
                page_size: query.page_size,
                outgoing_offset,
                incoming_offset,
                limits,
            },
            &before,
            |id, h, ordinal, r, heads| {
                page_edge(
                    id,
                    h,
                    ordinal,
                    r,
                    &PageContext {
                        generation: &generation,
                        heads,
                        original,
                    },
                )
            },
            |direction, offset| {
                original.check()?;
                Ok(MemoryReferencesCursor {
                    binding: Box::new(binding.clone()),
                    direction,
                    offset,
                })
            },
        )
        .await?;
        let page = |p: QueriedPage<MemoryReferenceEdge, MemoryReferencesCursor>| {
            Box::new(MemoryReferencePage {
                results: p.results,
                total: p.total,
                complete: p.complete,
                next_cursor: p.next_cursor,
            })
        };
        let warnings = result.warnings;
        let outgoing = result.outgoing.map(page);
        let incoming = result.incoming.map(page);
        let output = MemoryReferencesRead::Published {
            generation,
            outgoing,
            incoming,
            coverage: MemoryReferencesCoverage {
                project_id,
                memory_project_source_complete: warnings.is_empty(),
                inbound_other_sources_not_qualified: true,
            },
            warnings,
        };
        original.check()?;
        before()?;
        Ok(output)
    }
}

/// The immutable reference snapshot follows the domain revision, not a new
/// reference generation on content-only changes.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Projection {
    pub source: MemoryId,
    pub project_id: ProjectId,
    pub operation_version: String,
    pub revision: u64,
    pub reference_generation: u64,
    pub tombstone: bool,
    pub references: Vec<MemoryReference>,
}

#[derive(Serialize)]
struct BorrowedProjection<'a> {
    source: MemoryId,
    project_id: ProjectId,
    operation_version: &'a str,
    revision: u64,
    reference_generation: u64,
    tombstone: bool,
    references: &'a [MemoryReference],
}

struct EncodedBytes {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: bool,
}
impl io::Write for EncodedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(count) = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= self.maximum)
        else {
            self.exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Memory reference payload limit",
            ));
        };
        self.bytes
            .try_reserve(count - self.bytes.len())
            .map_err(io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn payload(
    project_id: ProjectId,
    operation_version: &str,
    memory: &Memory,
    tombstone: bool,
    limits: MemoryStoreWriteLimits,
) -> Result<String, MemoryStoreError> {
    let projection = BorrowedProjection {
        source: memory.id,
        project_id,
        operation_version,
        revision: memory.revision,
        reference_generation: memory.reference_generation,
        tombstone,
        references: &memory.references,
    };
    // Enforce the byte limit before every growth of the single encoded buffer.
    let mut bytes = EncodedBytes {
        bytes: Vec::new(),
        maximum: limits.max_row_bytes.get(),
        exceeded: false,
    };
    if let Err(error) = serde_json::to_writer(&mut bytes, &projection) {
        return if bytes.exceeded {
            Err(MemoryStoreError::Limit("Memory reference payload bytes"))
        } else {
            Err(error.into())
        };
    }
    String::from_utf8(bytes.bytes).map_err(|_| MemoryStoreError::Invalid("Memory reference UTF-8"))
}

pub(super) fn decode(
    batch: &RecordBatch,
    row: usize,
    original: &SnapshotContext,
    expected_project: ProjectId,
) -> Result<Projection, MemoryStoreError> {
    original.check()?;
    let raw = rows::text(batch, 17, row)?;
    let projection: Projection = serde_json::from_str(raw)?;
    if projection.project_id != expected_project
        || projection.revision == 0
        || projection.reference_generation == 0
        || projection.reference_generation > projection.revision
        || projection.operation_version.trim().is_empty()
        || (projection.tombstone && !projection.references.is_empty())
    {
        return Err(MemoryStoreError::Invalid(
            "Memory reference projection identity",
        ));
    }
    super::write::validate_references(original, expected_project, &projection.references)?;
    if rows::text(batch, 0, row)?
        != format!("references/{}/{}", projection.source, projection.revision)
        || rows::text(batch, 1, row)? != "references"
        || rows::text(batch, 2, row)? != projection.source.to_string()
        || rows::text(batch, 3, row)? != projection.project_id.to_string()
        || rows::text(batch, 4, row)? != projection.operation_version
        || rows::number(batch, 8, row)? != projection.revision
        || rows::number(batch, 9, row)? != 0
        || rows::number(batch, 10, row)? != 0
        || !rows::flag(batch, 13, row)?
        || rows::flag(batch, 14, row)? != projection.tombstone
        || rows::number(batch, 15, row)? != projection.reference_generation
        || rows::text(batch, 16, row)? != serde_json::to_string(&projection.references)?
        || raw != serde_json::to_string(&projection)?
    {
        return Err(MemoryStoreError::Invalid(
            "Memory reference projection header",
        ));
    }
    for column in [5, 6, 7, 11, 12] {
        if !rows::text(batch, column, row)?.is_empty() {
            return Err(MemoryStoreError::Invalid(
                "Memory reference projection text",
            ));
        }
    }
    original.check()?;
    Ok(projection)
}
