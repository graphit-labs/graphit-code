use super::{native, rows, *};
use crate::{
    memory_store::{self as domain, read, revision, write, *},
    reference_projection::LogicalScope,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use graphit_auth::{UserReadAdmission, UserWriteAdmission};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    lance_branch_store::{BranchSnapshot, DeltaRecoveryKey, DeltaResolution, JsonFtsIndexMode},
    lance_module_store::StoreError,
};
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU128,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn record_filter(id: MemoryId) -> String {
    format!("record_id = {}", literal(&id.to_string()))
}
fn attempt_filter(owner: &UserMemoryOwner, actor: &str, key: &str) -> String {
    format!(
        "installation_id = {} AND user_issuer = {} AND user_subject = {} AND idempotency_actor = {} AND idempotency_key = {}",
        literal(owner.installation_id.as_str()),
        literal(owner.identity.issuer()),
        literal(owner.identity.subject()),
        literal(actor),
        literal(key)
    )
}
pub(super) async fn scan(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    owner: &UserMemoryOwner,
    filter: &str,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<read::DecodedRows<rows::Row>, MemoryStoreError> {
    read::scan_decoded(
        pin,
        filter,
        rows::user_memory_schema().as_ref(),
        original,
        limits,
        before,
        |batch, row| rows::decode(batch, row, original, owner),
    )
    .await
}
pub(crate) struct Chain {
    pub creation: UserCreationMemory,
    pub current: Memory,
    pub deleted: bool,
    pub revisions: Vec<UserMemoryRevision>,
}
pub(crate) fn assemble(
    found: Vec<rows::Row>,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<Chain>, MemoryStoreError> {
    if found.is_empty() {
        before()?;
        return Ok(None);
    }
    let (mut head, mut creation) = (None, None);
    let mut events = vec![];
    let mut attempts = HashSet::new();
    for row in found {
        before()?;
        if row.kind != "head" && !attempts.insert((row.actor.clone(), row.key.clone())) {
            return Err(MemoryStoreError::Invalid("duplicate USER chain attempt"));
        }
        match row.kind.as_str() {
            "head" if head.is_none() => head = Some(row),
            "creation" if creation.is_none() => creation = Some(row),
            "revision" => events.push(row.revision.ok_or(MemoryStoreError::IncompleteCorpus)?),
            _ => return Err(MemoryStoreError::Invalid("duplicate USER chain member")),
        }
    }
    let (head, audit) = head
        .zip(creation)
        .ok_or(MemoryStoreError::IncompleteCorpus)?;
    if head.key != audit.key || head.actor != audit.actor {
        return Err(MemoryStoreError::Invalid("USER creation binding disagrees"));
    }
    let creation = audit.creation.ok_or(MemoryStoreError::IncompleteCorpus)?;
    events.sort_by_key(|e| e.after.revision);
    let mut current = &creation.memory;
    let mut deleted = false;
    for event in &events {
        before()?;
        if deleted {
            return Err(MemoryStoreError::Invalid("USER revision after deletion"));
        }
        if current.revision.checked_add(1) != Some(event.after.revision) {
            return Err(MemoryStoreError::IncompleteCorpus);
        }
        if event.before != *current {
            return Err(MemoryStoreError::Invalid("USER history fork"));
        }
        current = &event.after;
        deleted = event.operation.deleted();
    }
    if head.memory != *current || head.tombstone != deleted {
        return Err(MemoryStoreError::Invalid("USER current/history disagrees"));
    }
    let current = current.clone();
    before()?;
    Ok(Some(Chain {
        creation,
        current,
        deleted,
        revisions: events,
    }))
}
pub(super) async fn chain(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    owner: &UserMemoryOwner,
    id: MemoryId,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<Chain>, MemoryStoreError> {
    let found = scan(pin, original, owner, &record_filter(id), limits, &before).await?;
    assemble(found.logical, before)
}
pub(super) async fn chains(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    owner: &UserMemoryOwner,
    limits: MemoryStoreReadLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<HashMap<MemoryId, Chain>, MemoryStoreError> {
    let found = scan(pin, original, owner, "true", limits, &before).await?;
    let mut groups: HashMap<MemoryId, Vec<rows::Row>> = HashMap::new();
    for row in found.logical {
        before()?;
        groups.entry(row.memory.id).or_default().push(row);
    }
    let mut result = HashMap::new();
    for (id, group) in groups {
        before()?;
        result.insert(
            id,
            assemble(group, &before)?.ok_or(MemoryStoreError::IncompleteCorpus)?,
        );
    }
    before()?;
    Ok(result)
}
async fn lookup_create(
    pin: &BranchSnapshot,
    owner: &UserMemoryOwner,
    actor: &str,
    original: &Arc<SnapshotContext>,
    input: &CreateMemory,
    limits: MemoryStoreWriteLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<Memory>, MemoryStoreError> {
    let found = scan(
        pin,
        original,
        owner,
        &attempt_filter(owner, actor, &input.idempotency_key),
        limits.read,
        &before,
    )
    .await?;
    if found.logical.is_empty() {
        before()?;
        return Ok(None);
    }
    let c = chain(
        pin,
        original,
        owner,
        found.logical[0].memory.id,
        limits.read,
        &before,
    )
    .await?
    .ok_or(MemoryStoreError::IncompleteCorpus)?;
    if found.logical.len() != 2
        || !found
            .logical
            .iter()
            .all(|r| r.kind == "head" || r.kind == "creation")
    {
        return Err(MemoryStoreError::IdempotencyConflict);
    }
    if c.creation.input != *input || c.creation.actor != actor {
        return Err(MemoryStoreError::IdempotencyConflict);
    }
    if found
        .logical
        .iter()
        .any(|r| r.memory.id != c.creation.memory.id)
    {
        return Err(MemoryStoreError::Invalid(
            "USER creation observation disagrees",
        ));
    }
    before()?;
    Ok(Some(c.creation.memory))
}
pub(super) async fn lookup_mutation(
    capture: &native::WritePin,
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    id: MemoryId,
    operation: &MemoryMutation,
    limits: MemoryStoreWriteLimits,
) -> Result<Option<UserMemoryRevision>, MemoryStoreError> {
    let before = || capture.check(original, admission);
    let actor = native::actor(admission)?;
    let event = lookup_attempt(
        &capture.pin,
        original,
        &capture.owner,
        &actor,
        operation.key(),
        limits,
        &before,
    )
    .await?;
    match event {
        Some(event) if event.after.id == id && event.operation == *operation => Ok(Some(event)),
        Some(_) => Err(MemoryStoreError::IdempotencyConflict),
        None => Ok(None),
    }
}
pub(super) async fn lookup_attempt(
    pin: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
    owner: &UserMemoryOwner,
    actor: &str,
    key: &str,
    limits: MemoryStoreWriteLimits,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<UserMemoryRevision>, MemoryStoreError> {
    let found = scan(
        pin,
        original,
        owner,
        &attempt_filter(owner, actor, key),
        limits.read,
        &before,
    )
    .await?;
    if found.logical.is_empty() {
        before()?;
        return Ok(None);
    }
    if found.logical.len() != 1 {
        return Err(MemoryStoreError::IdempotencyConflict);
    }
    let row = found
        .logical
        .into_iter()
        .next()
        .ok_or(MemoryStoreError::IncompleteCorpus)?;
    let event = row.revision.ok_or(MemoryStoreError::IdempotencyConflict)?;
    let c = chain(pin, original, owner, event.after.id, limits.read, &before)
        .await?
        .ok_or(MemoryStoreError::IncompleteCorpus)?;
    if !c.revisions.contains(&event) {
        return Err(MemoryStoreError::Invalid(
            "USER mutation observation disagrees",
        ));
    }
    before()?;
    Ok(Some(event))
}
pub(super) async fn publish(
    pending: graphit_data::lance_branch_store::PreparedDelta,
    owner: &UserMemoryOwner,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<UserMemoryPublication, MemoryStoreError> {
    let recovery_key = pending.recovery_key().clone();
    before()?;
    let pin = pending.commit().await?;
    let publication = UserMemoryPublication {
        generation: native::generation(&pin, owner),
        recovery_key,
    };
    before().map_err(|cause| MemoryStoreError::UserConfirmed {
        publication: Box::new(publication.clone()),
        cause: Box::new(cause),
    })?;
    Ok(publication)
}
async fn publish_index(
    prepared: graphit_data::lance_branch_store::PreparedIndex,
    owner: &UserMemoryOwner,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<(BranchSnapshot, UserMemoryLexicalIndexPublication), MemoryStoreError> {
    let recovery_key = prepared.recovery_key().clone();
    before()?;
    let pin = prepared.commit().await?;
    let publication = UserMemoryLexicalIndexPublication {
        generation: native::generation(&pin, owner),
        recovery_key,
    };
    before().map_err(|cause| MemoryStoreError::UserIndexConfirmed {
        publication: Box::new(publication.clone()),
        cause: Box::new(cause),
    })?;
    Ok((pin, publication))
}

async fn initialize_lexical_index(
    capture: &mut native::WritePin,
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    limits: MemoryStoreWriteLimits,
) -> Result<Option<UserMemoryLexicalIndexPublication>, MemoryStoreError> {
    use graphit_data::lance_branch_store::JsonFtsIndexState;
    let before = || capture.check(original, admission);
    before()?;
    let state = capture.pin.json_fts_index_state("payload").await?;
    before()?;
    match state {
        JsonFtsIndexState::Ready => return Ok(None),
        JsonFtsIndexState::Incompatible => {
            return Err(MemoryStoreError::LexicalIndexUnavailable(state));
        }
        JsonFtsIndexState::Missing => {}
    }
    chains(&capture.pin, original, &capture.owner, limits.read, &before).await?;
    before()?;
    let prepared = capture
        .pin
        .prepare_json_fts_index(
            "payload",
            "memory-json-content-v1",
            JsonFtsIndexMode::Create,
        )
        .await?;
    let (pin, publication) = publish_index(prepared, &capture.owner, before).await?;
    capture.pin = pin;
    Ok(Some(publication))
}

fn preserve_index_publication<T>(
    result: Result<T, MemoryStoreError>,
    publication: Option<UserMemoryLexicalIndexPublication>,
) -> Result<T, MemoryStoreError> {
    result.map_err(|error| {
        let Some(publication) = publication else {
            return error;
        };
        match error {
            MemoryStoreError::UserConfirmed {
                publication: memory,
                cause,
            } => MemoryStoreError::UserConfirmed {
                publication: memory,
                cause: Box::new(MemoryStoreError::UserIndexConfirmed {
                    publication: Box::new(publication),
                    cause,
                }),
            },
            cause => MemoryStoreError::UserIndexConfirmed {
                publication: Box::new(publication),
                cause: Box::new(cause),
            },
        }
    })
}
pub(super) fn now() -> Result<u64, MemoryStoreError> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| MemoryStoreError::Invalid("clock precedes epoch"))?
            .as_millis(),
    )
    .map_err(|_| MemoryStoreError::Invalid("timestamp overflow"))
}
fn ordinary(operation: &MemoryMutation) -> Result<(), MemoryStoreError> {
    match operation {
        MemoryMutation::Update(_)
        | MemoryMutation::Delete(_)
        | MemoryMutation::Promote(_)
        | MemoryMutation::Demote(_) => Ok(()),
        _ => Err(MemoryStoreError::WrongAdmission),
    }
}
impl NativeUserMemoryStore {
    pub async fn create(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        input: CreateMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        create_with_policy(
            original,
            admission,
            input,
            limits,
            native::UserWritePolicy::Ordinary,
        )
        .await
    }

    pub async fn reobserve_create(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        input: CreateMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<MemoryCreateObservation, MemoryStoreError> {
        reobserve_create_with_policy(
            original,
            admission,
            input,
            limits,
            native::UserWritePolicy::Ordinary,
        )
        .await
    }

    pub async fn update(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        id: MemoryId,
        input: UpdateMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        mutate(
            original,
            admission,
            id,
            MemoryMutation::Update(input),
            limits,
        )
        .await
    }
    pub async fn delete(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        id: MemoryId,
        input: DeleteMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        mutate(
            original,
            admission,
            id,
            MemoryMutation::Delete(input),
            limits,
        )
        .await
    }
    pub async fn promote(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        id: MemoryId,
        input: ChangeMemoryImportance,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        mutate(
            original,
            admission,
            id,
            MemoryMutation::Promote(input),
            limits,
        )
        .await
    }
    pub async fn demote(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        id: MemoryId,
        input: ChangeMemoryImportance,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        mutate(
            original,
            admission,
            id,
            MemoryMutation::Demote(input),
            limits,
        )
        .await
    }
    pub async fn reobserve_mutation(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        id: MemoryId,
        operation: MemoryMutation,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Option<UserMemoryRevision>, MemoryStoreError> {
        native::check_write(original, admission)?;
        ordinary(&operation)?;
        let owner = native::owner(admission.user()?.live)?;
        let sid = UserMemoryScopeId::from_owner(&owner)?;
        revision::validate_in_scope(
            original,
            LogicalScope::User(sid.as_str()),
            &operation,
            limits.max_input_bytes.get(),
        )?;
        let Some(capture) = native::acquire(original, admission, limits, false).await? else {
            native::check_write(original, admission)?;
            return Ok(None);
        };
        lookup_mutation(&capture, original, admission, id, &operation, limits).await
    }
    pub async fn get(
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
        id: MemoryId,
        limits: MemoryStoreReadLimits,
    ) -> Result<Option<UserMemoryDetail>, MemoryStoreError> {
        let Some(capture) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return Ok(None);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
        };
        let c = chain(&capture.pin, original, &capture.owner, id, limits, &before).await?;
        before()?;
        Ok(c.filter(|c| !c.deleted).map(|c| UserMemoryDetail {
            memory: c.current,
            creation: c.creation,
            generation: native::generation(&capture.pin, &capture.owner),
        }))
    }
    pub async fn history(
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
        id: MemoryId,
        limits: MemoryStoreReadLimits,
    ) -> Result<Option<UserMemoryHistory>, MemoryStoreError> {
        let Some(capture) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return Ok(None);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
        };
        let c = chain(&capture.pin, original, &capture.owner, id, limits, &before).await?;
        before()?;
        Ok(c.map(|c| UserMemoryHistory {
            creation: c.creation,
            current: c.current,
            deleted: c.deleted,
            revisions: c.revisions,
            generation: native::generation(&capture.pin, &capture.owner),
        }))
    }
    pub async fn list(
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
        filter: MemoryListFilter,
        limits: MemoryStoreReadLimits,
    ) -> Result<UserMemoryListRead, MemoryStoreError> {
        native::check_read(original, admission)?;
        if let Some(tag) = &filter.tag {
            domain::nonblank(tag)?;
        }
        let Some(capture) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return Ok(UserMemoryListRead::NoCorpus);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
        };
        let found = scan(
            &capture.pin,
            original,
            &capture.owner,
            "kind = 'head' AND superseded = false AND tombstone = false",
            limits,
            &before,
        )
        .await?;
        let mut memories: Vec<_> = found
            .logical
            .into_iter()
            .map(|r| r.memory)
            .filter(|m| filter.matches(m))
            .collect();
        memories.sort_by(|a, b| {
            domain::catalogue::category(a)
                .cmp(&domain::catalogue::category(b))
                .then_with(|| b.updated_at.cmp(&a.updated_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        before()?;
        Ok(UserMemoryListRead::Published {
            generation: native::generation(&capture.pin, &capture.owner),
            memories,
        })
    }
    pub async fn observe_heads(
        original: &Arc<SnapshotContext>,
        admission: &UserReadAdmission<'_>,
        limits: MemoryHeadReadLimits,
    ) -> Result<UserMemoryHeadRead, MemoryStoreError> {
        let Some(capture) = native::read_pin(original, admission, limits.native).await? else {
            native::check_read(original, admission)?;
            return Ok(UserMemoryHeadRead::NoCorpus);
        };
        let before = || {
            native::check_read(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, MemoryStoreError>(())
        };
        let names = [
            "record_id",
            "installation_id",
            "user_issuer",
            "user_subject",
            "title",
            "updated_by",
            "updated_at",
        ];
        let expected = Schema::new_with_metadata(
            names
                .into_iter()
                .enumerate()
                .map(|(i, n)| {
                    Field::new(
                        n,
                        if i == 6 {
                            DataType::UInt64
                        } else {
                            DataType::Utf8
                        },
                        false,
                    )
                })
                .collect::<Vec<_>>(),
            rows::user_memory_schema().metadata().clone(),
        );
        let mut scanner = capture.pin.scan();
        before()?;
        scanner
            .filter("kind = 'head' AND superseded = false AND tombstone = false")
            .map_err(StoreError::from)?;
        scanner.project(&names).map_err(StoreError::from)?;
        scanner.batch_size(limits.batch_rows.get() as usize);
        let mut stream = crate::storage::read(original, scanner.try_into_stream())
            .await?
            .map_err(StoreError::from)?;
        before()?;
        let mut heads = vec![];
        let mut ids = HashSet::new();
        loop {
            before()?;
            let batch = crate::storage::read(original, stream.try_next())
                .await?
                .map_err(StoreError::from)?;
            before()?;
            let Some(batch) = batch else { break };
            domain::rows::validate_schema(&batch, &expected)?;
            for r in 0..batch.num_rows() {
                before()?;

                if domain::rows::text(&batch, 1, r)? != capture.owner.installation_id.as_str()
                    || domain::rows::text(&batch, 2, r)? != capture.owner.identity.issuer()
                    || domain::rows::text(&batch, 3, r)? != capture.owner.identity.subject()
                {
                    return Err(MemoryStoreError::Invalid("foreign USER head owner"));
                }
                let id = domain::rows::text(&batch, 0, r)?.parse()?;
                if !ids.insert(id) {
                    return Err(MemoryStoreError::Invalid("duplicate USER head ID"));
                }
                let title = domain::rows::text(&batch, 4, r)?;
                let actor = domain::rows::text(&batch, 5, r)?;
                domain::nonblank(title)?;
                domain::valid_actor(actor)?;
                heads.push(MemoryActivityHead {
                    id,
                    title: title.into(),
                    updated_by: actor.into(),
                    updated_at: domain::rows::number(&batch, 6, r)?,
                });
            }
        }
        heads.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        before()?;
        Ok(UserMemoryHeadRead::Published {
            generation: native::generation(&capture.pin, &capture.owner),
            heads,
        })
    }
    pub async fn maintain_lexical_index(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        operation: MemoryLexicalIndexOperation,
        limits: MemoryStoreWriteLimits,
    ) -> Result<UserMemoryLexicalIndexOutcome, MemoryStoreError> {
        use graphit_data::lance_branch_store::JsonFtsIndexState;
        let Some(capture) = native::acquire(original, admission, limits, false).await? else {
            native::check_write(original, admission)?;
            return Ok(UserMemoryLexicalIndexOutcome::NoCorpus);
        };
        let before = || capture.check(original, admission);
        before()?;
        let state = capture.pin.json_fts_index_state("payload").await?;
        before()?;
        let mode = match (operation, state) {
            (MemoryLexicalIndexOperation::Ensure, JsonFtsIndexState::Ready) => {
                return Ok(UserMemoryLexicalIndexOutcome::Ready(native::generation(
                    &capture.pin,
                    &capture.owner,
                )));
            }
            (MemoryLexicalIndexOperation::Ensure, JsonFtsIndexState::Missing) => {
                JsonFtsIndexMode::Create
            }
            (MemoryLexicalIndexOperation::Refresh, JsonFtsIndexState::Ready) => {
                JsonFtsIndexMode::Replace
            }
            (_, state) => return Err(MemoryStoreError::LexicalIndexUnavailable(state)),
        };
        chains(&capture.pin, original, &capture.owner, limits.read, &before).await?;
        before()?;
        let prepared = capture
            .pin
            .prepare_json_fts_index("payload", "memory-json-content-v1", mode)
            .await?;
        let (_, publication) = publish_index(prepared, &capture.owner, before).await?;
        Ok(UserMemoryLexicalIndexOutcome::Published(publication))
    }
    pub async fn resolve_lexical_index(
        original: &Arc<SnapshotContext>,
        admission: &UserWriteAdmission<'_>,
        key: &DeltaRecoveryKey,
        limits: MemoryStoreWriteLimits,
    ) -> Result<DeltaResolution, MemoryStoreError> {
        native::check_write(original, admission)?;
        let corpus_uri = match admission.record_storage().await? {
            Some(storage) => storage.uri,
            None => native::uri(admission.user()?.live)?,
        };
        if key.corpus_uri() != corpus_uri || key.physical_branch().is_some() {
            return Err(graphit_data::lance_branch_store::BranchError::RecoveryBinding.into());
        }
        let Some(capture) = native::acquire(original, admission, limits, false).await? else {
            native::check_write(original, admission)?;
            return Ok(DeltaResolution::Unresolved);
        };
        capture.check(original, admission)?;
        let result = capture.pin.resolve(key).await?;
        capture.check(original, admission)?;
        Ok(result)
    }
}
async fn mutate(
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    id: MemoryId,
    operation: MemoryMutation,
    limits: MemoryStoreWriteLimits,
) -> Result<Memory, MemoryStoreError> {
    mutate_with_policy(
        original,
        admission,
        id,
        operation,
        limits,
        native::UserWritePolicy::Ordinary,
    )
    .await
}
pub(super) async fn mutate_with_policy(
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    id: MemoryId,
    operation: MemoryMutation,
    limits: MemoryStoreWriteLimits,
    policy: native::UserWritePolicy,
) -> Result<Memory, MemoryStoreError> {
    policy.check(original, admission)?;
    ordinary(&operation)?;
    let owner = native::owner(admission.user()?.live)?;
    let sid = UserMemoryScopeId::from_owner(&owner)?;
    let scope = LogicalScope::User(sid.as_str());
    revision::validate_in_scope(original, scope, &operation, limits.max_input_bytes.get())?;
    let mut capture =
        native::acquire_with_check(original, admission, limits, false, policy, || Ok(()))
            .await?
            .ok_or(MemoryStoreError::NotFound)?;
    if let Some(event) =
        lookup_mutation(&capture, original, admission, id, &operation, limits).await?
    {
        return Ok(event.after);
    }
    let before = || capture.check(original, admission);
    let c = chain(&capture.pin, original, &owner, id, limits.read, &before)
        .await?
        .filter(|c| !c.deleted)
        .ok_or(MemoryStoreError::NotFound)?;
    if operation.expected() != c.current.revision {
        return Err(MemoryStoreError::RevisionConflict {
            expected: operation.expected(),
            actual: c.current.revision,
        });
    }
    if matches!(&operation, MemoryMutation::Promote(_)) && c.current.attributes.important
        || matches!(&operation, MemoryMutation::Demote(_)) && !c.current.attributes.important
    {
        before()?;
        return Ok(c.current);
    }
    let after = revision::derive_in_scope(
        original,
        scope,
        &c.current,
        &operation,
        &native::actor(admission)?,
        now()?,
    )?;
    let event = UserMemoryRevision {
        owner,
        actor: native::actor(admission)?,
        operation,
        before: c.current,
        after: after.clone(),
    };
    let batch = rows::mutation_batch(&c.creation, &event, limits)?;
    before()?;
    let index = initialize_lexical_index(&mut capture, original, admission, limits).await?;
    let result = async {
        let before = || capture.check(original, admission);
        before()?;
        let pending = capture
            .pin
            .prepare_merge(vec![batch], vec!["row_key".into()])
            .await?;
        publish(pending, &capture.owner, before).await
    }
    .await;
    preserve_index_publication(result, index)?;
    Ok(after)
}

#[cfg(test)]
#[path = "../../tests/user_memory_store/publication.rs"]
mod publication_tests;

pub(super) async fn create_with_policy(
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    input: CreateMemory,
    limits: MemoryStoreWriteLimits,
    policy: native::UserWritePolicy,
) -> Result<Memory, MemoryStoreError> {
    policy.check(original, admission)?;
    write::input_budget(original, &input, limits.max_input_bytes.get())?;
    let owner = native::owner(admission.user()?.live)?;
    let scope_id = UserMemoryScopeId::from_owner(&owner)?;
    let references =
        write::validate_input_in_scope(original, LogicalScope::User(scope_id.as_str()), &input)?;
    let mut capture =
        native::acquire_with_check(original, admission, limits, true, policy, || Ok(()))
            .await?
            .ok_or(MemoryStoreError::IncompleteCorpus)?;
    if let Some(existing) = lookup_create(
        &capture.pin,
        &capture.owner,
        &native::actor(admission)?,
        original,
        &input,
        limits,
        || capture.check(original, admission),
    )
    .await?
    {
        return Ok(existing);
    }
    let id = MemoryId::new(
        NonZeroU128::new(ulid::Ulid::generate().into())
            .ok_or(MemoryStoreError::Invalid("generated USER Memory ID"))?,
    );
    let before = || capture.check(original, admission);
    if scan(
        &capture.pin,
        original,
        &owner,
        &record_filter(id),
        limits.read,
        &before,
    )
    .await?
    .has_rows
    {
        return Err(MemoryStoreError::Invalid("USER Memory ID collision"));
    }
    let at = now()?;
    let actor = native::actor(admission)?;
    let memory = Memory {
        id,
        revision: 1,
        title: input.title.clone(),
        body: input.body.clone(),
        attributes: input.attributes,
        tags: input.tags.clone(),
        references,
        reference_generation: 1,
        created_by: actor.clone(),
        updated_by: actor.clone(),
        created_at: at,
        updated_at: at,
    };
    let event = UserCreationMemory {
        owner,
        actor,
        input,
        memory: memory.clone(),
    };
    let batch = rows::creation_batch(&event, limits)?;
    before()?;
    let index = initialize_lexical_index(&mut capture, original, admission, limits).await?;
    let result = async {
        let before = || capture.check(original, admission);
        before()?;
        let pending = capture
            .pin
            .prepare_merge(vec![batch], vec!["row_key".into()])
            .await?;
        publish(pending, &capture.owner, before).await
    }
    .await;
    preserve_index_publication(result, index)?;
    Ok(memory)
}

pub(super) async fn reobserve_create_with_policy(
    original: &Arc<SnapshotContext>,
    admission: &UserWriteAdmission<'_>,
    input: CreateMemory,
    limits: MemoryStoreWriteLimits,
    policy: native::UserWritePolicy,
) -> Result<MemoryCreateObservation, MemoryStoreError> {
    policy.check(original, admission)?;
    write::input_budget(original, &input, limits.max_input_bytes.get())?;
    let owner = native::owner(admission.user()?.live)?;
    let sid = UserMemoryScopeId::from_owner(&owner)?;
    write::validate_input_in_scope(original, LogicalScope::User(sid.as_str()), &input)?;
    let Some(capture) = native::read_write_pin(original, admission, limits.read, policy).await?
    else {
        policy.check(original, admission)?;
        return Ok(MemoryCreateObservation::Unresolved);
    };
    Ok(
        match lookup_create(
            &capture.pin,
            &capture.owner,
            &native::actor(admission)?,
            original,
            &input,
            limits,
            || {
                policy.check(original, admission)?;
                capture.guard.revalidate(original)?;
                Ok(())
            },
        )
        .await?
        {
            Some(m) => MemoryCreateObservation::Found(m),
            None => MemoryCreateObservation::Unresolved,
        },
    )
}
