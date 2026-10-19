use super::{model::kind, native, read, rows, *};
use crate::reference_projection::{
    AuthoredReferenceEntry, AuthoredReferenceUpdate, AuthoredReferencesField, LogicalScope,
    ReferenceCandidate, SourceNamespace, project_authored_references,
};
use graphit_auth::ProjectWriteAdmission;
use graphit_core::{
    artifact_store::{ArtifactStore, ArtifactWriter},
    file_io::SnapshotContext,
    project_identity::ProjectId,
};
use graphit_data::lance_branch_store::{BranchSnapshot, LanceBranchStore};
use std::{
    num::NonZeroU128,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn validate_references(
    original: &SnapshotContext,
    project: ProjectId,
    refs: &[MemoryReference],
) -> Result<Vec<MemoryReference>, MemoryStoreError> {
    validate_references_in_scope(original, LogicalScope::Project(project), refs)
}
pub(crate) fn validate_references_in_scope(
    original: &SnapshotContext,
    scope: LogicalScope<'_>,
    refs: &[MemoryReference],
) -> Result<Vec<MemoryReference>, MemoryStoreError> {
    let scope_ids: Vec<String> = refs
        .iter()
        .map(|r| match &r.scope {
            MemoryReferenceScope::Project(p) => p.to_string(),
            MemoryReferenceScope::User(u) => u.clone(),
        })
        .collect();
    let candidates: Vec<_> = refs
        .iter()
        .zip(&scope_ids)
        .map(|(r, scope_id)| {
            AuthoredReferenceEntry::Tuple(ReferenceCandidate {
                target_type: kind(r.target_kind),
                id: &r.id,
                scope: Some(match r.scope {
                    MemoryReferenceScope::Project(_) => "project",
                    MemoryReferenceScope::User(_) => "user",
                }),
                scope_id: Some(scope_id),
                context: r.context.as_deref(),
                relation: &r.relation,
                field: r.field.as_deref(),
            })
        })
        .collect();
    let AuthoredReferenceUpdate::Replace(projected) = project_authored_references(
        original,
        SourceNamespace {
            scope,
            context: None,
        },
        AuthoredReferencesField::Sequence(&candidates),
    )?
    else {
        return Err(MemoryStoreError::Invalid("reference projection"));
    };
    Ok(projected
        .into_iter()
        .map(|r| MemoryReference {
            target_kind: r.target_kind,
            scope: match r.scope {
                LogicalScope::Project(p) => MemoryReferenceScope::Project(p),
                LogicalScope::User(u) => MemoryReferenceScope::User(u.to_owned()),
            },
            id: r.id.to_owned(),
            context: r.context.map(str::to_owned),
            relation: r.relation.to_owned(),
            field: r.field.map(str::to_owned),
        })
        .collect())
}
pub(super) fn validate_input(
    original: &SnapshotContext,
    project: ProjectId,
    input: &CreateMemory,
) -> Result<Vec<MemoryReference>, MemoryStoreError> {
    validate_input_in_scope(original, LogicalScope::Project(project), input)
}
pub(crate) fn validate_input_in_scope(
    original: &SnapshotContext,
    scope: LogicalScope<'_>,
    input: &CreateMemory,
) -> Result<Vec<MemoryReference>, MemoryStoreError> {
    original.check()?;
    for text in [&input.idempotency_key, &input.title, &input.body] {
        nonblank(text)?;
    }
    for tag in &input.tags {
        original.check()?;
        nonblank(tag)?;
    }
    match &input.references {
        None => Ok(Vec::new()),
        Some(refs) => validate_references_in_scope(original, scope, refs),
    }
}
pub(crate) fn input_budget(
    original: &SnapshotContext,
    input: &CreateMemory,
    maximum: usize,
) -> Result<(), MemoryStoreError> {
    let mut total = 0;
    for text in [&input.idempotency_key, &input.title, &input.body]
        .into_iter()
        .chain(input.tags.iter())
    {
        original.check()?;
        charge(&mut total, text.len(), maximum, "authored input bytes")?;
    }
    if let Some(refs) = &input.references {
        for r in refs {
            original.check()?;
            let project_scope;
            let scope = match &r.scope {
                MemoryReferenceScope::Project(p) => {
                    project_scope = p.to_string();
                    project_scope.as_str()
                }
                MemoryReferenceScope::User(u) => u.as_str(),
            };
            for text in [kind(r.target_kind), scope, &r.id, &r.relation]
                .into_iter()
                .chain(r.context.as_deref())
                .chain(r.field.as_deref())
            {
                charge(&mut total, text.len(), maximum, "authored input bytes")?;
            }
        }
    }
    original.check()?;
    Ok(())
}
pub(super) fn actor(admission: &ProjectWriteAdmission<'_>) -> Result<String, MemoryStoreError> {
    let mut bytes = [0; 64];
    admission.actor()?.write_identifier(&mut bytes);
    String::from_utf8(bytes.to_vec()).map_err(|_| MemoryStoreError::Invalid("actor encoding"))
}
pub(super) struct WritePin {
    pub pin: BranchSnapshot,
    pub guard: crate::storage::Guard,
    _writer: Option<ArtifactWriter>,
    policy: native::WritePolicy,
}
impl WritePin {
    pub fn check(
        &self,
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
    ) -> Result<(), MemoryStoreError> {
        self.policy.check(original, admission)?;
        self.guard.revalidate(original)?;
        Ok(())
    }
}
pub(super) async fn acquire(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: MemoryStoreWriteLimits,
    create: bool,
) -> Result<Option<WritePin>, MemoryStoreError> {
    acquire_with_policy(
        original,
        admission,
        limits,
        create,
        native::WritePolicy::Ordinary,
    )
    .await
}
pub(super) async fn acquire_with_policy(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: MemoryStoreWriteLimits,
    create: bool,
    policy: native::WritePolicy,
) -> Result<Option<WritePin>, MemoryStoreError> {
    acquire_with_check(original, admission, limits, create, policy, || Ok(())).await
}
pub(super) async fn acquire_with_check(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: MemoryStoreWriteLimits,
    create: bool,
    policy: native::WritePolicy,
    paired: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<Option<WritePin>, MemoryStoreError> {
    policy.check(original, admission)?;
    paired()?;
    let project = admission.project()?;
    let selected = native::Selection::resolve(
        admission.record_storage().await?,
        project.scope.live,
        project.version.version(),
        limits.read.native,
        original,
    )?;
    acquire_selected_with_check(
        original, admission, limits, create, policy, paired, selected,
    )
    .await
}
pub(super) async fn acquire_selected(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: MemoryStoreWriteLimits,
    selected: native::Selection,
) -> Result<Option<WritePin>, MemoryStoreError> {
    acquire_selected_with_check(
        original,
        admission,
        limits,
        false,
        native::WritePolicy::Ordinary,
        || Ok(()),
        selected,
    )
    .await
}
async fn acquire_selected_with_check(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: MemoryStoreWriteLimits,
    create: bool,
    policy: native::WritePolicy,
    paired: impl Fn() -> Result<(), MemoryStoreError>,
    selected: native::Selection,
) -> Result<Option<WritePin>, MemoryStoreError> {
    let check = || {
        policy.check(original, admission)?;
        paired()
    };
    check()?;
    let project = admission.project()?;
    let scope = project.scope.live;
    let (relative, uri) = match selected {
        native::Selection::Remote(storage) => {
            check()?;
            storage.check()?;
            let opened =
                crate::storage::open_remote(original, &storage, rows::schema(), create).await;
            check()?;
            let Some(store) = opened? else {
                check()?;
                return Ok(None);
            };
            let branch = project.version.lance_branch();
            let pin = crate::storage::read(
                original,
                store.capture_target((!branch.is_empty()).then_some(branch.as_str())),
            )
            .await??;
            check()?;
            let pin = if create {
                pin.materialize().await?
            } else {
                pin
            };
            check()?;
            storage.check()?;
            return Ok(Some(WritePin {
                pin,
                guard: crate::storage::Guard::Remote(storage),
                _writer: None,
                policy,
            }));
        }
        native::Selection::Local { relative, uri } => (relative, uri),
    };
    let artifacts = ArtifactStore::new(scope.clone(), limits.artifacts);
    let request = Arc::clone(original);
    let mut writer = tokio::task::spawn_blocking(move || artifacts.writer_with_context(&request))
        .await
        .map_err(MemoryStoreError::WriterAcquisition)??;
    check()?;
    let guard = match native::capture(scope, &relative, original, limits.read.native, true, check)?
    {
        Some(guard) => guard,
        None => {
            let parent =
                native::capture(scope, &relative, original, limits.read.native, false, check)?;
            if let Some(parent) = &parent {
                check()?;
                parent.revalidate(original)?;
                return Err(MemoryStoreError::IncompleteCorpus);
            }
            if !create {
                return Ok(None);
            }
            birth(&mut writer, &uri, original, admission, limits, policy).await?;
            native::capture(scope, &relative, original, limits.read.native, true, check)?
                .ok_or(MemoryStoreError::IncompleteCorpus)?
        }
    };
    let before = || {
        check()?;
        guard.revalidate(original)?;
        Ok::<_, MemoryStoreError>(())
    };
    before()?;
    let store = LanceBranchStore::open(&uri, rows::schema(), None).await?;
    before()?;
    let branch = project.version.lance_branch();
    let pin = crate::storage::read(
        original,
        store.capture_target((!branch.is_empty()).then_some(branch.as_str())),
    )
    .await??;
    before()?;
    let pin = if create {
        let pin = pin.materialize().await?;
        before()?;
        pin
    } else {
        pin
    };
    before()?;
    Ok(Some(WritePin {
        pin,
        guard: crate::storage::Guard::Project(guard),
        _writer: Some(writer),
        policy,
    }))
}
async fn birth(
    writer: &mut ArtifactWriter,
    uri: &str,
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: MemoryStoreWriteLimits,
    policy: native::WritePolicy,
) -> Result<(), MemoryStoreError> {
    use graphit_core::artifact_store::ProjectRecordDirectory;
    use graphit_data::lance_module_store::LanceModuleStore;
    policy.check(original, admission)?;
    let parent =
        writer.create_record_directory(ProjectRecordDirectory::Memory, limits.read.native)?;
    policy.check(original, admission)?;
    parent.revalidate(original)?;
    LanceModuleStore::create(
        uri,
        rows::schema(),
        vec![arrow_array::RecordBatch::new_empty(rows::schema())],
    )
    .await?;
    policy.check(original, admission)?;
    parent.revalidate(original)?;
    Ok(())
}
pub(super) fn attempt_filter(
    project_id: ProjectId,
    version: &str,
    actor: &str,
    key: &str,
) -> String {
    format!(
        "project_id = {} AND operation_version = {} AND idempotency_actor = {} AND idempotency_key = {}",
        native::literal(&project_id.to_string()),
        native::literal(version),
        native::literal(actor),
        native::literal(key)
    )
}
async fn lookup(
    capture: &WritePin,
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    input: &CreateMemory,
    limits: MemoryStoreWriteLimits,
) -> Result<Option<Memory>, MemoryStoreError> {
    let project = admission.project()?;
    let project_id = native::project_id(project.scope.live)?;
    let actor = actor(admission)?;
    let before = || {
        capture.check(original, admission)?;
        Ok::<_, MemoryStoreError>(())
    };
    // A surviving head must not conceal a missing or corrupt creation event.
    let filter = attempt_filter(
        project_id,
        project.version.version(),
        &actor,
        &input.idempotency_key,
    );
    let members = read::exact_rows(
        &capture.pin,
        &filter,
        original,
        project_id,
        limits.read,
        &before,
    )
    .await?;
    if members.is_empty() {
        before()?;
        return Ok(None);
    }
    let chain = super::history::chain(
        &capture.pin,
        original,
        members[0].memory.id,
        project_id,
        limits.read,
        &before,
    )
    .await?
    .ok_or(MemoryStoreError::IncompleteCorpus)?;
    if members.len() == 1 && members[0].revision.is_some() {
        return Err(MemoryStoreError::IdempotencyConflict);
    }
    if members.len() != 2
        || !members.iter().any(|r| r.kind == "head")
        || !members.iter().any(|r| r.kind == "creation")
    {
        return Err(MemoryStoreError::IncompleteCorpus);
    }
    let event = chain.creation;
    if event.input != *input {
        return Err(MemoryStoreError::IdempotencyConflict);
    }
    if members.iter().any(|row| row.memory.id != event.memory.id) {
        return Err(MemoryStoreError::Invalid("creation observation disagrees"));
    }
    before()?;
    Ok(Some(event.memory))
}
impl NativeMemoryStore {
    /// Publish one current record and its immutable initial audit/reference marker.
    pub async fn create(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        input: CreateMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<Memory, MemoryStoreError> {
        Self::create_with_policy(
            original,
            admission,
            input,
            limits,
            native::WritePolicy::Ordinary,
        )
        .await
    }
    pub(super) async fn create_with_policy(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        input: CreateMemory,
        limits: MemoryStoreWriteLimits,
        policy: native::WritePolicy,
    ) -> Result<Memory, MemoryStoreError> {
        policy.check(original, admission)?;
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        input_budget(original, &input, limits.max_input_bytes.get())?;
        let references = validate_input(original, project_id, &input)?;
        policy.check(original, admission)?;
        let mut capture = acquire_with_policy(original, admission, limits, true, policy)
            .await?
            .ok_or(MemoryStoreError::IncompleteCorpus)?;
        if let Some(existing) = lookup(&capture, original, admission, &input, limits).await? {
            return Ok(existing);
        }
        let id = MemoryId::new(
            NonZeroU128::new(ulid::Ulid::generate().into())
                .ok_or(MemoryStoreError::Invalid("generated Memory ID"))?,
        );
        let before = || {
            capture.check(original, admission)?;
            Ok::<_, MemoryStoreError>(())
        };
        // Count physical rows before logical dispatch; a derived row also reserves the ID.
        read::ensure_id_available(&capture.pin, id, original, project_id, limits.read, &before)
            .await?;
        let at = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| MemoryStoreError::Invalid("clock precedes epoch"))?
                .as_millis(),
        )
        .map_err(|_| MemoryStoreError::Invalid("timestamp overflow"))?;
        let actor = actor(admission)?;
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
        let event = CreationMemory {
            project_id,
            operation_version: project.version.version().to_owned(),
            actor,
            input,
            memory: memory.clone(),
        };
        let batch = rows::batch(&event, limits)?;
        before()?;
        let index =
            super::lexical_index::initialize(&mut capture, original, admission, limits).await?;
        let result = async {
            let before = || capture.check(original, admission);
            before()?;
            let pending = capture
                .pin
                .prepare_merge(vec![batch], vec!["row_key".into()])
                .await?;
            publish(pending, before).await
        }
        .await;
        super::lexical_index::preserve_publication(result, index)?;
        Ok(memory)
    }
    /// Reobserve the same actor/version/key with fresh authority, without any SDK write.
    pub async fn reobserve_create(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        input: CreateMemory,
        limits: MemoryStoreWriteLimits,
    ) -> Result<MemoryCreateObservation, MemoryStoreError> {
        Self::reobserve_create_with_policy(
            original,
            admission,
            input,
            limits,
            native::WritePolicy::Ordinary,
        )
        .await
    }
    pub(super) async fn reobserve_create_with_policy(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        input: CreateMemory,
        limits: MemoryStoreWriteLimits,
        policy: native::WritePolicy,
    ) -> Result<MemoryCreateObservation, MemoryStoreError> {
        policy.check(original, admission)?;
        input_budget(original, &input, limits.max_input_bytes.get())?;
        validate_input(
            original,
            native::project_id(admission.project()?.scope.live)?,
            &input,
        )?;
        let Some(capture) = acquire_with_policy(original, admission, limits, false, policy).await?
        else {
            policy.check(original, admission)?;
            return Ok(MemoryCreateObservation::Unresolved);
        };
        match lookup(&capture, original, admission, &input, limits).await? {
            Some(memory) => Ok(MemoryCreateObservation::Found(memory)),
            None => Ok(MemoryCreateObservation::Unresolved),
        }
    }
}

pub(super) async fn publish(
    pending: graphit_data::lance_branch_store::PreparedDelta,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<(), MemoryStoreError> {
    publish_version(pending, before).await.map(|_| ())
}
pub(super) async fn publish_version(
    pending: graphit_data::lance_branch_store::PreparedDelta,
    before: impl Fn() -> Result<(), MemoryStoreError>,
) -> Result<u64, MemoryStoreError> {
    before()?;
    let published = pending.commit().await?;
    before().map_err(|cause| MemoryStoreError::Confirmed {
        version: published.version(),
        cause: Box::new(cause),
    })?;
    Ok(published.version())
}
#[cfg(all(test, target_os = "linux"))]
pub(super) mod tests {
    include!("../../tests/memory_store/boundaries.rs");
}
