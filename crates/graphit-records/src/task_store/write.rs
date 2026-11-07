use super::{
    NativeTaskStore, TaskStoreError, TaskStoreWriteLimits, model::*, native, read, references, rows,
};
use crate::{
    gates::TaskStatus,
    task_spec::{CheckId, CheckRecord, CheckStatus, TaskId, TaskSpecification},
};
use graphit_auth::ProjectWriteAdmission;
use graphit_core::{
    artifact_store::{ArtifactStore, ArtifactWriter, ProjectRecordDirectory},
    file_io::SnapshotContext,
};
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::LanceModuleStore};
use std::{
    collections::HashSet,
    num::NonZeroU128,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn fresh() -> Result<NonZeroU128, TaskStoreError> {
    NonZeroU128::new(ulid::Ulid::generate().into())
        .ok_or(TaskStoreError::Invalid("generated identifier"))
}
pub(super) fn now() -> Result<u64, TaskStoreError> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| TaskStoreError::Invalid("clock precedes epoch"))?
            .as_millis(),
    )
    .map_err(|_| TaskStoreError::Invalid("timestamp overflow"))
}
pub(super) fn actor(admission: &ProjectWriteAdmission<'_>) -> Result<String, TaskStoreError> {
    Ok(admission.task_actor()?.identifier().to_string())
}
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
pub(super) struct WritePin {
    pub pin: BranchSnapshot,
    pub guard: crate::storage::Guard,
    read_limits: super::TaskStoreReadLimits,
    _writer: Option<ArtifactWriter>,
}
pub(super) async fn acquire(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: TaskStoreWriteLimits,
    allow_birth: bool,
) -> Result<WritePin, TaskStoreError> {
    acquire_inner(original, admission, limits, allow_birth, true).await
}
pub(super) async fn acquire_unmaterialized(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: TaskStoreWriteLimits,
) -> Result<WritePin, TaskStoreError> {
    acquire_inner(original, admission, limits, false, false).await
}
async fn acquire_inner(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: TaskStoreWriteLimits,
    allow_birth: bool,
    materialize: bool,
) -> Result<WritePin, TaskStoreError> {
    native::check_write(original, admission)?;
    let project = admission.project()?;
    let selected = native::Selection::resolve(
        admission.record_storage().await?,
        project.scope.live,
        project.version.version(),
        limits.read.native,
        original,
    )?;
    acquire_selected(
        original,
        admission,
        limits,
        allow_birth,
        materialize,
        selected,
    )
    .await
}
pub(super) async fn acquire_selected(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: TaskStoreWriteLimits,
    allow_birth: bool,
    materialize: bool,
    selected: native::Selection,
) -> Result<WritePin, TaskStoreError> {
    native::check_write(original, admission)?;
    let project = admission.project()?;
    let scope = project.scope.live;
    let (relative, uri) = match selected {
        native::Selection::Remote(storage) => {
            native::check_write(original, admission)?;
            storage.check()?;
            let branch = project.version.lance_branch();
            let pin = native::remote_pin(
                original,
                &storage,
                (!branch.is_empty()).then_some(branch.as_str()),
                allow_birth,
            )
            .await?
            .ok_or(TaskStoreError::MissingSession)?;
            native::check_write(original, admission)?;
            let pin = if materialize {
                pin.materialize().await?
            } else {
                pin
            };
            native::check_write(original, admission)?;
            storage.check()?;
            return Ok(WritePin {
                pin,
                guard: crate::storage::Guard::Remote(storage),
                read_limits: limits.read,
                _writer: None,
            });
        }
        native::Selection::Local { relative, uri } => (relative, uri),
    };
    let artifacts = ArtifactStore::new(scope.clone(), limits.artifacts);
    let request = Arc::clone(original);
    let mut writer = tokio::task::spawn_blocking(move || artifacts.writer_with_context(&request))
        .await
        .map_err(TaskStoreError::WriterAcquisition)??;
    native::check_write(original, admission)?;
    let guard = match native::capture(scope, &relative, original, limits.read, true, || {
        native::check_write(original, admission)
    })? {
        Some(guard) => guard,
        None => {
            if !allow_birth {
                return Err(TaskStoreError::MissingSession);
            }
            let parent =
                match native::capture(scope, &relative, original, limits.read, false, || {
                    native::check_write(original, admission)
                })? {
                    Some(parent) => {
                        native::check_write(original, admission)?;
                        parent.revalidate(original)?;
                        return Err(TaskStoreError::IncompleteCorpus);
                    }
                    None => writer.create_record_directory(
                        ProjectRecordDirectory::Task,
                        limits.read.native,
                    )?,
                };
            native::check_write(original, admission)?;
            parent.revalidate(original)?;
            LanceModuleStore::create(
                &uri,
                rows::schema(),
                vec![arrow_array::RecordBatch::new_empty(rows::schema())],
            )
            .await?;
            native::check_write(original, admission)?;
            parent.revalidate(original)?;
            native::capture(scope, &relative, original, limits.read, true, || {
                native::check_write(original, admission)
            })?
            .ok_or(TaskStoreError::IncompleteCorpus)?
        }
    };
    let before = || {
        native::check_write(original, admission)?;
        guard.revalidate(original)?;
        Ok::<_, TaskStoreError>(())
    };
    before()?;
    let branch = project.version.lance_branch();
    let pin = crate::storage::read(
        original,
        BranchSnapshot::capture_observed(
            &uri,
            (!branch.is_empty()).then_some(branch.as_str()),
            None,
        ),
    )
    .await??;
    rows::validate_schema(pin.schema())?;
    before()?;
    let pin = if materialize {
        pin.materialize().await?
    } else {
        pin
    };
    before()?;
    Ok(WritePin {
        pin,
        guard: crate::storage::Guard::Project(guard),
        read_limits: limits.read,
        _writer: Some(writer),
    })
}
pub(super) async fn materialize(
    capture: WritePin,
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    owner_lease_expires_at: Option<u64>,
) -> Result<WritePin, TaskStoreError> {
    let WritePin {
        pin,
        guard,
        read_limits,
        _writer,
    } = capture;
    let before = || {
        native::check_write(original, admission)?;
        guard.revalidate(original)?;
        if let Some(end) = owner_lease_expires_at
            && now()? >= end
        {
            return Err(TaskStoreError::LeaseExpired);
        }
        Ok::<_, TaskStoreError>(())
    };
    before()?;
    let pin = pin.materialize().await?;
    before()?;
    Ok(WritePin {
        pin,
        guard,
        read_limits,
        _writer,
    })
}
pub(super) async fn commit(
    mut capture: WritePin,
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    rows: Vec<rows::Row>,
    owner_lease_expires_at: Option<u64>,
) -> Result<(), TaskStoreError> {
    let before = || {
        native::check_write(original, admission)?;
        capture.guard.revalidate(original)?;
        if let Some(expires) = owner_lease_expires_at
            && now()? >= expires
        {
            return Err(TaskStoreError::LeaseExpired);
        }
        Ok::<_, TaskStoreError>(())
    };
    before()?;
    let batch = rows::batch_for(&rows, capture.pin.schema())?;
    before()?;
    let index =
        super::lexical_index::initialize(&mut capture.pin, original, capture.read_limits, &before)
            .await?;
    let result = async {
        before()?;
        let pending = capture
            .pin
            .prepare_merge(vec![batch], vec!["row_key".into()])
            .await?;
        before()?;
        let published = pending.commit().await?;
        after_commit(published.version(), before())
    }
    .await;
    super::lexical_index::preserve_publication(result, index)
}
fn after_commit(
    version: u64,
    revalidation: Result<(), TaskStoreError>,
) -> Result<(), TaskStoreError> {
    revalidation.map_err(|cause| TaskStoreError::Confirmed {
        version,
        cause: Box::new(cause),
    })
}
impl NativeTaskStore {
    /// Create a session and its immutable initial history in one strict SDK commit.
    pub async fn create_session(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        input: CreateSession,
        references: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Session, TaskStoreError> {
        native::require_operation(admission, &[graphit_auth::ProjectWriteOperation::Task])?;
        native::check_write(original, admission)?;
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let references_update =
            references::authored(original, project_id, references, limits.references)?;
        let project_version = project.version.version().to_owned();
        for text in [
            &input.idempotency_key,
            &input.title,
            &input.description,
            &input.strategy,
        ] {
            super::canonical(text)?;
        }
        let capture = acquire(original, admission, limits, true).await?;
        let before = || {
            native::check_write(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        let filter = format!(
            "kind = 'session' AND idempotency_key = {}",
            literal(&input.idempotency_key)
        );
        let found = read::exact_rows(&capture.pin, &filter, original, limits.read, before).await?;
        if !found.is_empty() {
            if found.len() != 1 {
                return Err(TaskStoreError::Invalid("duplicate session idempotency"));
            }
            let rows::Row::Session(existing) = found.into_iter().next().unwrap() else {
                return Err(TaskStoreError::Invalid("session row"));
            };
            let existing = existing.record;
            let creation_rows = read::exact_rows(
                &capture.pin,
                &format!(
                    "kind = 'session_creation' AND record_id = '{}'",
                    id(existing.id.get())
                ),
                original,
                limits.read,
                before,
            )
            .await?;
            let [rows::Row::SessionCreation(creation)] = creation_rows.as_slice() else {
                return Err(TaskStoreError::Invalid("session creation identity"));
            };
            if creation.record.input != input || creation.references_update != references_update {
                return Err(TaskStoreError::IdempotencyConflict);
            }
            before()?;
            return Ok(existing);
        }
        let at = now()?;
        let session = Session {
            id: SessionId::new(fresh()?),
            input,
            revision: 1,
            references: references_update.initial(),
            reference_generation: 1,
            status: TaskStatus::Open,
            ownership: Ownership::default(),
            progress: None,
            created_at: at,
            updated_at: at,
        };
        let creation = Creation {
            actor: actor(admission)?,
            record: session.clone(),
            references_update: references_update.clone(),
        };
        commit(
            capture,
            original,
            admission,
            vec![
                rows::Row::Session(rows::Stored {
                    record: session.clone(),
                    fence: None,
                }),
                rows::Row::SessionCreation(creation),
                rows::Row::ReferenceProjection(references::session_projection(
                    &session,
                    project_id,
                    &project_version,
                )),
            ],
            None,
        )
        .await?;
        Ok(session)
    }
    /// Create a fresh task. Claim/fencing and lifecycle mutations have separate APIs.
    pub async fn create_task(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        input: CreateTask,
        references: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Task, TaskStoreError> {
        native::require_operation(admission, &[graphit_auth::ProjectWriteOperation::Task])?;
        native::check_write(original, admission)?;
        let project = admission.project()?;
        let project_id = native::project_id(project.scope.live)?;
        let references_update =
            references::authored(original, project_id, references, limits.references)?;
        let project_version = project.version.version().to_owned();
        super::canonical(&input.idempotency_key)?;
        if let Some(commit) = &input.commit {
            graphit_core::git_record_event::validate_commit_id(commit).map_err(|_| {
                TaskStoreError::Invalid("expected a full lowercase Git commit object ID")
            })?;
        }
        let spec = TaskSpecification {
            id: TaskId::new(fresh()?),
            revision: 1,
            title: input.title.clone(),
            description: input.description.clone(),
            task_type: input.task_type.clone(),
            priority: input.priority,
            schedule: input.schedule,
            checks: input
                .checks
                .iter()
                .map(|c| {
                    Ok(CheckRecord {
                        id: CheckId::new(fresh()?),
                        kind: c.kind,
                        text: c.text.clone(),
                        status: CheckStatus::Pending,
                        verification: None,
                        supersession: None,
                    })
                })
                .collect::<Result<_, TaskStoreError>>()?,
        };
        spec.validate(original)?;
        let mut dependencies = HashSet::new();
        if input.depends_on.iter().any(|d| !dependencies.insert(*d)) {
            return Err(TaskStoreError::Invalid("duplicate dependency"));
        }
        let capture = acquire(original, admission, limits, false).await?;
        let before = || {
            native::check_write(original, admission)?;
            capture.guard.revalidate(original)?;
            Ok::<_, TaskStoreError>(())
        };
        let filter = format!(
            "kind = 'task' AND idempotency_key = {}",
            literal(&input.idempotency_key)
        );
        let found = read::exact_rows(&capture.pin, &filter, original, limits.read, before).await?;
        if !found.is_empty() {
            if found.len() != 1 {
                return Err(TaskStoreError::Invalid("duplicate task idempotency"));
            }
            let rows::Row::Task(existing) = found.into_iter().next().unwrap() else {
                return Err(TaskStoreError::Invalid("task row"));
            };
            let existing = existing.record;
            let creation_rows = read::exact_rows(
                &capture.pin,
                &format!(
                    "kind = 'task_creation' AND record_id = '{}'",
                    id(existing.specification.id.get())
                ),
                original,
                limits.read,
                before,
            )
            .await?;
            let [rows::Row::TaskCreation(creation)] = creation_rows.as_slice() else {
                return Err(TaskStoreError::Invalid("task creation identity"));
            };
            let initial = &creation.record;
            let observed = CreateTask {
                commit: initial.commit.clone(),
                idempotency_key: initial.idempotency_key.clone(),
                session_id: initial.session_id,
                parent_id: initial.parent_id,
                depends_on: initial.depends_on.clone(),
                title: initial.specification.title.clone(),
                description: initial.specification.description.clone(),
                task_type: initial.specification.task_type.clone(),
                priority: initial.specification.priority,
                schedule: initial.specification.schedule,
                checks: existing
                    .specification
                    .checks
                    .iter()
                    .map(|c| CreateCheck {
                        kind: c.kind,
                        text: c.text.clone(),
                    })
                    .collect(),
            };
            if observed != input || creation.references_update != references_update {
                return Err(TaskStoreError::IdempotencyConflict);
            }
            before()?;
            return Ok(existing);
        }
        let session_filter = format!(
            "kind = 'session' AND record_id = '{}'",
            id(input.session_id.get())
        );
        let sessions =
            read::exact_rows(&capture.pin, &session_filter, original, limits.read, before).await?;
        if sessions.len() != 1 {
            return Err(TaskStoreError::MissingSession);
        }
        let rows::Row::Session(session) = &sessions[0] else {
            return Err(TaskStoreError::Invalid("session row"));
        };
        if session.record.status != TaskStatus::Open
            && session.record.status != TaskStatus::InProgress
        {
            return Err(TaskStoreError::Invalid("closed session"));
        }
        let at = now()?;
        let task = Task {
            commit: input.commit,
            specification: spec,
            session_id: input.session_id,
            parent_id: input.parent_id,
            depends_on: input.depends_on,
            references: references_update.initial(),
            reference_generation: 1,
            idempotency_key: input.idempotency_key,
            status: TaskStatus::Open,
            flagged: false,
            ownership: Ownership::default(),
            progress: None,
            created_at: at,
            updated_at: at,
        };
        read::validate_relations(&capture.pin, &task, original, limits.read, before).await?;
        let creation = Creation {
            actor: actor(admission)?,
            record: task.clone(),
            references_update: references_update.clone(),
        };
        commit(
            capture,
            original,
            admission,
            vec![
                rows::Row::Task(rows::Stored {
                    record: task.clone(),
                    fence: None,
                }),
                rows::Row::TaskCreation(creation),
                rows::Row::ReferenceProjection(references::task_projection(
                    &task,
                    project_id,
                    &project_version,
                )),
            ],
            None,
        )
        .await?;
        Ok(task)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_fixture::*;
    use super::*;
    use crate::task_store::{TaskReferenceRecord, TaskReferencesQuery, TaskReferencesRead};
    use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
    use graphit_core::file_lock::Cancellation;
    use std::time::Duration;
    // Compose the private production post-commit boundary with a real SDK
    // publication and a real cancelled admission; this does not intercept SDK IO.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn comments_known_sdk_publication_keeps_causal_version_and_fresh_retry() {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "main");
        let issuer = f.issuer(&ctx);
        let secret =
            std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let write = authority
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Task)
            .unwrap();
        let read = authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
            .unwrap();
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            CreateSession {
                idempotency_key: "session".into(),
                title: "Session".into(),
                description: "Comments".into(),
                strategy: "Implement".into(),
            },
            crate::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let task = NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            CreateTask {
                commit: None,
                idempotency_key: "task".into(),
                session_id: session.id,
                parent_id: None,
                depends_on: vec![],
                title: "Task".into(),
                description: "Comments".into(),
                task_type: "feature".into(),
                priority: 2,
                schedule: None,
                checks: vec![
                    CreateCheck {
                        kind: crate::task_spec::CheckKind::Acceptance,
                        text: "Required comment behavior".into(),
                    },
                    CreateCheck {
                        kind: crate::task_spec::CheckKind::Test,
                        text: "Affected real SDK fixture".into(),
                    },
                ],
            },
            crate::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let claim = NativeTaskStore::claim_task(
            ctx.snapshot(),
            &write,
            task.specification.id,
            Duration::from_secs(60),
            write_limits(),
        )
        .await
        .unwrap();
        let input = || TaskCommentUpdate {
            claim: ClaimUpdate {
                token: &claim.token,
                expected_revision: claim.record.specification.revision,
            },
            lease: Duration::from_secs(30),
            idempotency_key: "comment".into(),
            kind: TaskCommentKind::Note,
            body: "Published".into(),
        };
        let entries = [crate::reference_projection::AuthoredReferenceEntry::Tuple(
            crate::reference_projection::ReferenceCandidate {
                target_type: "memory",
                id: "confirmed",
                relation: "supports",
                scope: None,
                scope_id: None,
                context: None,
                field: None,
            },
        )];
        let created = NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            task.specification.id,
            input(),
            crate::reference_projection::AuthoredReferencesField::Sequence(&entries),
            write_limits(),
        )
        .await
        .unwrap();
        let TaskCommentOutcome::Created { comment, .. } = created else {
            panic!("created")
        };
        let TaskReadView::Published { generation, .. } =
            NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
                .await
                .unwrap()
        else {
            panic!("generation")
        };
        cancel.cancel();
        let error = after_commit(
            generation.version,
            native::check_write(ctx.snapshot(), &write),
        )
        .unwrap_err();
        let TaskStoreError::Confirmed { version, cause } = error else {
            panic!("confirmed")
        };
        assert_eq!(version, generation.version);
        assert!(matches!(
            *cause,
            TaskStoreError::Auth(_) | TaskStoreError::Request(_)
        ));
        let fresh_cancel = Cancellation::default();
        let fresh = request(&fresh_cancel);
        let caller = issuer.authenticate_runtime(&secret, &fresh).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let write = authority
            .admit_authenticated_write(&caller, &fresh, f.scope(), ProjectWriteOperation::OwnTask)
            .unwrap();
        let read = authority
            .admit_authenticated_read(&caller, &fresh, f.scope(), ProjectReadModule::Task)
            .unwrap();
        let outcome = NativeTaskStore::add_task_comment(
            fresh.snapshot(),
            &write,
            task.specification.id,
            input(),
            crate::reference_projection::AuthoredReferencesField::Sequence(&entries),
            write_limits(),
        )
        .await
        .unwrap();
        let TaskCommentOutcome::Existing {
            comment: observed, ..
        } = outcome
        else {
            panic!("recovery")
        };
        assert_eq!(observed, comment);
        let TaskReadView::Published {
            generation: observed,
            ..
        } = NativeTaskStore::read_view(fresh.snapshot(), &read, read_limits())
            .await
            .unwrap()
        else {
            panic!("generation")
        };
        assert_eq!(observed, generation);
        let TaskReferencesRead::Published {
            generation: reference_pin,
            outgoing,
            ..
        } = NativeTaskStore::references(
            fresh.snapshot(),
            &read,
            TaskReferencesQuery {
                focus: TaskReferenceRecord::Task(task.specification.id),
                page_size: std::num::NonZeroUsize::new(20).unwrap(),
                outgoing_cursor: None,
                incoming_cursor: None,
            },
            read_limits(),
        )
        .await
        .unwrap()
        else {
            panic!("references observed")
        };
        assert_eq!(reference_pin, generation);
        assert!(outgoing.complete);
        assert_eq!(outgoing.results.last().unwrap().reference.id, "confirmed");
    }
}
