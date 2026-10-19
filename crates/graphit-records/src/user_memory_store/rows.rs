use super::*;
use crate::memory_store::{
    self as domain, Memory, MemoryStoreError, MemoryStoreWriteLimits,
    model::{StoredInput, StoredMutation},
    revision,
    rows::{self as shared, EncodedRow, MemoryLayout, RowOwner, schema_for},
    write,
};
use crate::reference_projection::LogicalScope;
use arrow_array::{Array, RecordBatch};
use arrow_schema::Schema;
use graphit_core::file_io::SnapshotContext;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Exact USER25 Arrow layout, including the six nullable embedding columns.
///
/// This schema describes data and cannot authorize an SDK open or directory birth.
pub fn user_memory_schema() -> Arc<Schema> {
    schema_for(MemoryLayout::User)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCreation {
    owner: UserMemoryOwner,
    actor: String,
    input: StoredInput,
    memory: Memory,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRevision {
    owner: UserMemoryOwner,
    actor: String,
    operation: StoredMutation,
    before: Memory,
    after: Memory,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Marker {
    pub source: domain::MemoryId,
    pub owner: UserMemoryOwner,
    pub revision: u64,
    pub reference_generation: u64,
    pub tombstone: bool,
    pub references: Vec<domain::MemoryReference>,
}
#[derive(Serialize)]
struct MarkerRef<'a> {
    source: domain::MemoryId,
    owner: &'a UserMemoryOwner,
    revision: u64,
    reference_generation: u64,
    tombstone: bool,
    references: &'a [domain::MemoryReference],
}
pub(crate) struct Row {
    pub kind: String,
    pub key: String,
    pub actor: String,
    pub memory: Memory,
    pub creation: Option<UserCreationMemory>,
    pub revision: Option<UserMemoryRevision>,
    pub tombstone: bool,
}
fn owner_header(
    batch: &RecordBatch,
    row: usize,
    owner: &UserMemoryOwner,
) -> Result<(), MemoryStoreError> {
    if shared::text(batch, 3, row)? != owner.installation_id.as_str()
        || shared::text(batch, 4, row)? != owner.identity.issuer()
        || shared::text(batch, 5, row)? != owner.identity.subject()
    {
        return Err(MemoryStoreError::Invalid("foreign USER Memory owner"));
    }
    Ok(())
}
pub(crate) fn decode(
    batch: &RecordBatch,
    row: usize,
    original: &SnapshotContext,
    owner: &UserMemoryOwner,
) -> Result<Option<Row>, MemoryStoreError> {
    original.check()?;
    owner_header(batch, row, owner)?;
    if shared::text(batch, 1, row)? == "embedding" {
        domain::embeddings::validate_row_for_layout(batch, row, shared::MemoryLayout::User)?;
        original.check()?;
        return Ok(None);
    }
    if batch.columns().iter().skip(19).any(|a| !a.is_null(row)) {
        return Err(MemoryStoreError::Invalid(
            "logical USER Memory embedding fields",
        ));
    }
    let kind = shared::text(batch, 1, row)?;
    if kind == "references" {
        decode_reference_marker(batch, row, original, owner)?;
        return Ok(None);
    }
    let scope_id = UserMemoryScopeId::from_owner(owner)?;
    let scope = LogicalScope::User(scope_id.as_str());
    let (memory, creation, event) = match kind {
        "head" => (
            serde_json::from_str::<Memory>(shared::text(batch, 18, row)?)?,
            None,
            None,
        ),
        "creation" => {
            let raw: StoredCreation = serde_json::from_str(shared::text(batch, 18, row)?)?;
            let event = UserCreationMemory {
                owner: raw.owner,
                actor: raw.actor,
                input: raw.input.into(),
                memory: raw.memory,
            };
            let refs = write::validate_input_in_scope(original, scope, &event.input)?;
            if event.owner != *owner
                || event.memory.title != event.input.title
                || event.memory.body != event.input.body
                || event.memory.attributes != event.input.attributes
                || event.memory.tags != event.input.tags
                || event.memory.references != refs
                || event.memory.created_by != event.actor
                || event.memory.revision != 1
            {
                return Err(MemoryStoreError::Invalid("USER creation input disagrees"));
            }
            (event.memory.clone(), Some(event), None)
        }
        "revision" => {
            let raw: StoredRevision = serde_json::from_str(shared::text(batch, 18, row)?)?;
            let event = UserMemoryRevision {
                owner: raw.owner,
                actor: raw.actor,
                operation: raw.operation.into(),
                before: raw.before,
                after: raw.after,
            };
            if event.owner != *owner {
                return Err(MemoryStoreError::Invalid("foreign USER audit owner"));
            }
            revision::validate_in_scope(original, scope, &event.operation, usize::MAX)?;
            shared::valid_memory_in_scope(original, scope, &event.before)?;
            shared::valid_memory_in_scope(original, scope, &event.after)?;
            if event.operation.expected() != event.before.revision
                || event.after.revision < 2
                || revision::derive_in_scope(
                    original,
                    scope,
                    &event.before,
                    &event.operation,
                    &event.actor,
                    event.after.updated_at,
                )? != event.after
            {
                return Err(MemoryStoreError::Invalid("USER revision outcome disagrees"));
            }
            (event.after.clone(), None, Some(event))
        }
        _ => return Err(MemoryStoreError::Invalid("unknown USER Memory row kind")),
    };
    shared::valid_memory_in_scope(original, scope, &memory)?;
    let actor = shared::text(batch, 7, row)?;
    domain::valid_actor(actor)?;
    let key = shared::text(batch, 6, row)?;
    domain::nonblank(key)?;
    let row_key = if kind == "revision" {
        format!("revision/{}/{}", memory.id, memory.revision)
    } else {
        format!("{kind}/{}", memory.id)
    };
    let tombstone = shared::flag(batch, 15, row)?;
    if shared::text(batch, 0, row)? != row_key
        || shared::text(batch, 2, row)? != memory.id.to_string()
        || shared::text(batch, 8, row)? != memory.title
        || shared::number(batch, 9, row)? != memory.revision
        || shared::number(batch, 10, row)? != memory.created_at
        || shared::number(batch, 11, row)? != memory.updated_at
        || shared::text(batch, 12, row)? != memory.created_by
        || shared::text(batch, 13, row)? != memory.updated_by
        || shared::flag(batch, 14, row)? != (kind != "head")
        || shared::number(batch, 16, row)? != memory.reference_generation
        || shared::text(batch, 17, row)? != serde_json::to_string(&memory.references)?
        || (kind != "revision" && actor != memory.created_by)
        || (kind == "creation" && tombstone)
    {
        return Err(MemoryStoreError::Invalid("USER Memory header disagrees"));
    }
    if let Some(e) = &creation
        && (e.input.idempotency_key != key || e.actor != actor)
    {
        return Err(MemoryStoreError::Invalid("USER creation binding disagrees"));
    }
    if let Some(e) = &event
        && (e.operation.key() != key
            || e.actor != actor
            || actor != memory.updated_by
            || e.operation.deleted() != tombstone)
    {
        return Err(MemoryStoreError::Invalid("USER revision binding disagrees"));
    }
    original.check()?;
    Ok(Some(Row {
        kind: kind.into(),
        key: key.into(),
        actor: actor.into(),
        memory,
        creation,
        revision: event,
        tombstone,
    }))
}
/// Complete private reference-family decoder; callers cannot bypass owner/header checks.
pub(crate) fn decode_reference_marker(
    batch: &RecordBatch,
    row: usize,
    original: &SnapshotContext,
    owner: &UserMemoryOwner,
) -> Result<Marker, MemoryStoreError> {
    original.check()?;
    shared::validate_schema(batch, user_memory_schema().as_ref())?;
    owner_header(batch, row, owner)?;
    if shared::text(batch, 1, row)? != "references"
        || batch.columns().iter().skip(19).any(|a| !a.is_null(row))
    {
        return Err(MemoryStoreError::Invalid("USER reference family/header"));
    }
    let scope_id = UserMemoryScopeId::from_owner(owner)?;
    decode_marker(
        batch,
        row,
        original,
        owner,
        LogicalScope::User(scope_id.as_str()),
    )
}
fn decode_marker(
    batch: &RecordBatch,
    row: usize,
    original: &SnapshotContext,
    owner: &UserMemoryOwner,
    scope: LogicalScope<'_>,
) -> Result<Marker, MemoryStoreError> {
    let raw = shared::text(batch, 18, row)?;
    let p: Marker = serde_json::from_str(raw)?;
    if p.owner != *owner
        || p.revision == 0
        || p.reference_generation == 0
        || p.reference_generation > p.revision
        || (p.tombstone && !p.references.is_empty())
    {
        return Err(MemoryStoreError::Invalid(
            "USER reference projection identity",
        ));
    }
    write::validate_references_in_scope(original, scope, &p.references)?;
    if shared::text(batch, 0, row)? != format!("references/{}/{}", p.source, p.revision)
        || shared::text(batch, 2, row)? != p.source.to_string()
        || shared::number(batch, 9, row)? != p.revision
        || shared::number(batch, 10, row)? != 0
        || shared::number(batch, 11, row)? != 0
        || !shared::flag(batch, 14, row)?
        || shared::flag(batch, 15, row)? != p.tombstone
        || shared::number(batch, 16, row)? != p.reference_generation
        || shared::text(batch, 17, row)? != serde_json::to_string(&p.references)?
        || raw != serde_json::to_string(&p)?
    {
        return Err(MemoryStoreError::Invalid(
            "USER reference projection header",
        ));
    }
    for c in [6, 7, 8, 12, 13] {
        if !shared::text(batch, c, row)?.is_empty() {
            return Err(MemoryStoreError::Invalid("USER reference projection text"));
        }
    }
    Ok(p)
}
fn marker(
    owner: &UserMemoryOwner,
    m: &Memory,
    tombstone: bool,
    limits: MemoryStoreWriteLimits,
) -> Result<String, MemoryStoreError> {
    shared::bounded_json(
        &MarkerRef {
            source: m.id,
            owner,
            revision: m.revision,
            reference_generation: m.reference_generation,
            tombstone,
            references: &m.references,
        },
        limits.max_row_bytes.get(),
    )
}
pub(super) fn creation_batch(
    event: &UserCreationMemory,
    limits: MemoryStoreWriteLimits,
) -> Result<RecordBatch, MemoryStoreError> {
    let owner = RowOwner::User(&event.owner);
    shared::encode(
        [
            EncodedRow {
                kind: "head",
                owner,
                key: &event.input.idempotency_key,
                actor: &event.actor,
                memory: &event.memory,
                payload: shared::bounded_json(&event.memory, limits.max_row_bytes.get())?,
                tombstone: false,
            },
            EncodedRow {
                kind: "creation",
                owner,
                key: &event.input.idempotency_key,
                actor: &event.actor,
                memory: &event.memory,
                payload: shared::bounded_json(event, limits.max_row_bytes.get())?,
                tombstone: false,
            },
            EncodedRow {
                kind: "references",
                owner,
                key: "",
                actor: "",
                memory: &event.memory,
                payload: marker(&event.owner, &event.memory, false, limits)?,
                tombstone: false,
            },
        ],
        limits,
    )
}
pub(super) fn mutation_batch(
    creation: &UserCreationMemory,
    event: &UserMemoryRevision,
    limits: MemoryStoreWriteLimits,
) -> Result<RecordBatch, MemoryStoreError> {
    let owner = RowOwner::User(&event.owner);
    let tombstone = event.operation.deleted();
    shared::encode(
        [
            EncodedRow {
                kind: "head",
                owner,
                key: &creation.input.idempotency_key,
                actor: &creation.actor,
                memory: &event.after,
                payload: shared::bounded_json(&event.after, limits.max_row_bytes.get())?,
                tombstone,
            },
            EncodedRow {
                kind: "revision",
                owner,
                key: event.operation.key(),
                actor: &event.actor,
                memory: &event.after,
                payload: shared::bounded_json(event, limits.max_row_bytes.get())?,
                tombstone,
            },
            EncodedRow {
                kind: "references",
                owner,
                key: "",
                actor: "",
                memory: &event.after,
                payload: marker(&event.owner, &event.after, tombstone, limits)?,
                tombstone,
            },
        ],
        limits,
    )
}
