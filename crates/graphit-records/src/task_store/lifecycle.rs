use super::{
    NativeTaskStore, TaskStoreError, TaskStoreWriteLimits, comments, model::*, native, read,
    references, rows, specification, write,
};
use crate::{gates::TaskStatus, task_spec::TaskId};
use graphit_auth::{ProjectWriteAdmission, ProjectWriteOperation};
use graphit_core::file_io::SnapshotContext;
use std::{sync::Arc, time::Duration};

#[derive(Clone, Copy)]
enum Target {
    Task(TaskId),
    Session(SessionId),
}
impl Target {
    fn id(self) -> String {
        match self {
            Self::Task(id) => model_id(id.get()),
            Self::Session(id) => model_id(id.get()),
        }
    }
    fn kind(self) -> &'static str {
        match self {
            Self::Task(_) => "task",
            Self::Session(_) => "session",
        }
    }
}
fn model_id(id: std::num::NonZeroU128) -> String {
    super::model::id(id)
}
enum Current {
    Task(rows::Stored<Task>),
    Session(rows::Stored<Session>),
}
impl Current {
    fn ownership(&self) -> &Ownership {
        match self {
            Self::Task(t) => &t.record.ownership,
            Self::Session(s) => &s.record.ownership,
        }
    }
    fn ownership_mut(&mut self) -> &mut Ownership {
        match self {
            Self::Task(t) => &mut t.record.ownership,
            Self::Session(s) => &mut s.record.ownership,
        }
    }
    fn fence(&self) -> Option<[u8; 32]> {
        match self {
            Self::Task(t) => t.fence,
            Self::Session(s) => s.fence,
        }
    }
    fn set_fence(&mut self, fence: Option<[u8; 32]>) {
        match self {
            Self::Task(t) => t.fence = fence,
            Self::Session(s) => s.fence = fence,
        }
    }
    fn revision(&self) -> u64 {
        match self {
            Self::Task(t) => t.record.specification.revision,
            Self::Session(s) => s.record.revision,
        }
    }
    fn updated_at(&self) -> u64 {
        match self {
            Self::Task(t) => t.record.updated_at,
            Self::Session(s) => s.record.updated_at,
        }
    }
    fn status(&self) -> TaskStatus {
        match self {
            Self::Task(t) => t.record.status,
            Self::Session(s) => s.record.status,
        }
    }
    fn progress_mut(&mut self) -> &mut Option<Progress> {
        match self {
            Self::Task(t) => &mut t.record.progress,
            Self::Session(s) => &mut s.record.progress,
        }
    }
    fn bump(&mut self, at: u64, status: TaskStatus) -> Result<(), TaskStoreError> {
        let revision = self
            .revision()
            .checked_add(1)
            .ok_or(TaskStoreError::Invalid("revision overflow"))?;
        if at < self.updated_at() {
            return Err(TaskStoreError::Invalid("clock moved backwards"));
        }
        match self {
            Self::Task(t) => {
                t.record.specification.revision = revision;
                t.record.reference_generation = revision;
                t.record.updated_at = at;
                t.record.status = status;
            }
            Self::Session(s) => {
                s.record.revision = revision;
                s.record.reference_generation = revision;
                s.record.updated_at = at;
                s.record.status = status;
            }
        }
        Ok(())
    }
    fn task_mut(&mut self) -> Result<&mut Task, TaskStoreError> {
        match self {
            Self::Task(t) => Ok(&mut t.record),
            _ => Err(TaskStoreError::Invalid("Task mutation target")),
        }
    }
    fn renew(&mut self, at: u64, lease: Duration) -> Result<(), TaskStoreError> {
        let end = expires(at, lease)?;
        let o = self.ownership_mut();
        o.heartbeat_at = Some(at);
        o.lease_expires_at = Some(
            o.lease_expires_at
                .ok_or(TaskStoreError::LeaseExpired)?
                .max(end),
        );
        Ok(())
    }
    fn clear_owner(&mut self) {
        let epoch = self.ownership().claim_epoch;
        *self.ownership_mut() = Ownership {
            claim_epoch: epoch,
            ..Ownership::default()
        };
        self.set_fence(None);
    }
    fn output(&self) -> Output {
        match self {
            Self::Task(t) => Output::Task(t.record.clone()),
            Self::Session(s) => Output::Session(s.record.clone()),
        }
    }
    fn publication(
        self,
        actor: String,
        kind: LifecycleKind,
        at: u64,
        references_update: references::TaskReferenceUpdate,
        project_id: graphit_core::project_identity::ProjectId,
        version: &str,
    ) -> Vec<rows::Row> {
        match self {
            Self::Task(t) => {
                let event = LifecycleEvent {
                    actor,
                    kind,
                    observed_at: at,
                    record: t.record.clone(),
                    references_update,
                };
                let projection = references::task_projection(&t.record, project_id, version);
                vec![
                    rows::Row::Task(t),
                    rows::Row::TaskEvent(event),
                    rows::Row::ReferenceProjection(projection),
                ]
            }
            Self::Session(s) => {
                let event = LifecycleEvent {
                    actor,
                    kind,
                    observed_at: at,
                    record: s.record.clone(),
                    references_update,
                };
                let projection = references::session_projection(&s.record, project_id, version);
                vec![
                    rows::Row::Session(s),
                    rows::Row::SessionEvent(event),
                    rows::Row::ReferenceProjection(projection),
                ]
            }
        }
    }
}
enum Output {
    Comment(Box<TaskCommentOutcome>),
    Task(Task),
    Session(Session),
}
impl Output {
    fn comment(self) -> TaskCommentOutcome {
        match self {
            Self::Comment(c) => *c,
            _ => unreachable!("typed comment target"),
        }
    }
    fn task(self) -> Task {
        match self {
            Self::Task(t) => t,
            _ => unreachable!("typed Task target"),
        }
    }
    fn session(self) -> Session {
        match self {
            Self::Session(s) => s,
            _ => unreachable!("typed Session target"),
        }
    }
}
enum Mutation<'a> {
    Comment(TaskCommentUpdate<'a>),
    Claim(Duration),
    Renew(ClaimUpdate<'a>, Duration),
    Progress(ProgressUpdate<'a>),
    Release(ReleaseUpdate<'a>),
    Specification(specification::Mutation<'a>),
    SessionRevision(SessionRevisionUpdate<'a>),
    Links(TaskLinksUpdate<'a>),
    Flag(TaskFlagUpdate<'a>),
    Commit(TaskCommitUpdate<'a>),
    Complete(ClaimUpdate<'a>),
    Cancel(TaskCancellation<'a>),
}
fn expires(at: u64, lease: Duration) -> Result<u64, TaskStoreError> {
    let ms =
        u64::try_from(lease.as_millis()).map_err(|_| TaskStoreError::Invalid("lease overflow"))?;
    if ms == 0 {
        return Err(TaskStoreError::Invalid("lease below one millisecond"));
    }
    at.checked_add(ms)
        .ok_or(TaskStoreError::Invalid("lease timestamp overflow"))
}
fn check_owner(
    current: &Current,
    actor: &str,
    claim: &ClaimUpdate<'_>,
    at: u64,
) -> Result<(), TaskStoreError> {
    if current.revision() != claim.expected_revision {
        return Err(TaskStoreError::RevisionConflict);
    }
    if current.ownership().owner.as_deref() != Some(actor) || current.fence() != Some(claim.token.0)
    {
        return Err(TaskStoreError::FenceMismatch);
    }
    if !current.ownership().live(at) {
        return Err(TaskStoreError::LeaseExpired);
    }
    Ok(())
}
async fn mutate(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    target: Target,
    mutation: Mutation<'_>,
    authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
    limits: TaskStoreWriteLimits,
) -> Result<(Output, Option<ClaimToken>), TaskStoreError> {
    native::check_write(original, admission)?;
    match target {
        Target::Task(_) => native::require_operation(
            admission,
            &[ProjectWriteOperation::OwnTask, ProjectWriteOperation::Task],
        )?,
        Target::Session(_) => {
            native::require_operation(admission, &[ProjectWriteOperation::CoordinateTask])?
        }
    };
    let project = admission.project()?;
    let project_id = native::project_id(project.scope.live)?;
    let project_version = project.version.version().to_owned();
    let references_update =
        references::authored(original, project_id, authored, limits.references)?;
    let defer_materialization = matches!(
        &mutation,
        Mutation::Comment(_) | Mutation::SessionRevision(_)
    );
    let capture = if defer_materialization {
        write::acquire_unmaterialized(original, admission, limits).await?
    } else {
        write::acquire(original, admission, limits, false).await?
    };
    let before = || {
        native::check_write(original, admission)?;
        capture.guard.revalidate(original)?;
        Ok::<_, TaskStoreError>(())
    };
    let filter = format!(
        "kind = '{}' AND record_id = '{}'",
        target.kind(),
        target.id()
    );
    let mut found = read::exact_rows(&capture.pin, &filter, original, limits.read, before).await?;
    if found.len() != 1 {
        return Err(match target {
            Target::Task(_) => TaskStoreError::MissingTask,
            Target::Session(_) => TaskStoreError::MissingSession,
        });
    }
    let mut current = match found.pop().unwrap() {
        rows::Row::Task(t) => Current::Task(t),
        rows::Row::Session(s) => Current::Session(s),
        _ => return Err(TaskStoreError::Invalid("current row kind")),
    };
    let actor = write::actor(admission)?;
    let at = write::now()?;
    if at < current.updated_at() {
        return Err(TaskStoreError::Invalid("clock moved backwards"));
    }
    let owner_lease_expires_at = if matches!(
        &mutation,
        Mutation::Claim(_) | Mutation::Commit(TaskCommitUpdate { claim: None, .. })
    ) {
        None
    } else {
        current.ownership().lease_expires_at
    };
    let mut added_comment = None;
    let kind = match mutation {
        Mutation::Comment(input) => {
            if current.status() != TaskStatus::InProgress {
                return Err(TaskStoreError::Invalid("comment requires in-progress task"));
            }
            let identity = ClaimUpdate {
                token: input.claim.token,
                expected_revision: current.revision(),
            };
            check_owner(&current, &actor, &identity, at)?;
            super::canonical(&input.idempotency_key)?;
            if input.body.trim().is_empty() {
                return Err(TaskStoreError::Invalid("blank comment body"));
            }
            expires(at, input.lease)?;
            let id = match target {
                Target::Task(id) => id,
                _ => return Err(TaskStoreError::Invalid("comment target")),
            };
            let detail = read::task_detail_at(&capture.pin, original, id, limits.read, before)
                .await?
                .ok_or(TaskStoreError::MissingTask)?;
            if let Some(c) = detail
                .comments
                .iter()
                .find(|c| c.idempotency_key == input.idempotency_key)
            {
                let event = comments::event(&detail.events, c)?;
                if current.ownership().claim_epoch != event.record.ownership.claim_epoch {
                    return Err(TaskStoreError::FenceMismatch);
                }
                if c.kind != input.kind
                    || c.body != input.body
                    || c.actor != actor
                    || event.references_update != references_update
                {
                    return Err(TaskStoreError::IdempotencyConflict);
                }
                if input.claim.expected_revision != current.revision()
                    && input.claim.expected_revision != c.revision - 1
                {
                    return Err(TaskStoreError::RevisionConflict);
                }
                before()?;
                check_owner(&current, &actor, &identity, write::now()?)?;
                let comment = c.clone();
                return Ok((
                    Output::Comment(Box::new(TaskCommentOutcome::Existing {
                        task: detail.task,
                        comment,
                    })),
                    None,
                ));
            }
            check_owner(&current, &actor, &input.claim, at)?;
            let sequence = std::num::NonZeroU64::new(
                u64::try_from(detail.comments.len())
                    .ok()
                    .and_then(|n| n.checked_add(1))
                    .ok_or(TaskStoreError::Invalid("comment sequence overflow"))?,
            )
            .ok_or(TaskStoreError::Invalid("comment sequence"))?;
            current.renew(at, input.lease)?;
            current.bump(at, TaskStatus::InProgress)?;
            let task = current.task_mut()?;
            let c = TaskComment {
                id: TaskCommentId::new(write::fresh()?),
                task_id: id,
                session_id: task.session_id,
                idempotency_key: input.idempotency_key,
                sequence,
                kind: input.kind,
                body: input.body,
                actor: actor.clone(),
                at,
                revision: task.specification.revision,
            };
            c.validate().map_err(TaskStoreError::Invalid)?;
            let kind = LifecycleKind::CommentAdded { id: c.id, sequence };
            added_comment = Some(c);
            kind
        }
        Mutation::Claim(lease) => {
            let end = expires(at, lease)?;
            if current.ownership().live(at) {
                if current.ownership().owner.as_deref() != Some(actor.as_str()) {
                    return Err(TaskStoreError::AlreadyClaimed);
                }
                before()?;
                let token = ClaimToken(
                    current
                        .fence()
                        .ok_or(TaskStoreError::Invalid("missing fence"))?,
                );
                return Ok((current.output(), Some(token)));
            }
            if !matches!(current.status(), TaskStatus::Open | TaskStatus::InProgress) {
                return Err(TaskStoreError::Invalid("terminal record cannot be claimed"));
            }
            if let Current::Task(t) = &current {
                let filter = format!(
                    "kind = 'session' AND record_id = '{}'",
                    model_id(t.record.session_id.get())
                );
                let sessions =
                    read::exact_rows(&capture.pin, &filter, original, limits.read, before).await?;
                if !matches!(sessions.as_slice(),[rows::Row::Session(s)] if matches!(s.record.status,TaskStatus::Open|TaskStatus::InProgress))
                {
                    return Err(TaskStoreError::Invalid("session is missing or terminal"));
                }
                read::require_ready(
                    &capture.pin,
                    t.record.specification.id,
                    original,
                    limits.read,
                    before,
                )
                .await?;
            }
            let epoch = current
                .ownership()
                .claim_epoch
                .checked_add(1)
                .ok_or(TaskStoreError::Invalid("claim epoch overflow"))?;
            let mut nonce = [0; 32];
            getrandom::fill(&mut nonce)?;
            *current.ownership_mut() = Ownership {
                owner: Some(actor.clone()),
                claim_epoch: epoch,
                claimed_at: Some(at),
                heartbeat_at: Some(at),
                lease_expires_at: Some(end),
            };
            current.set_fence(Some(nonce));
            current.bump(at, TaskStatus::InProgress)?;
            LifecycleKind::Claimed
        }
        Mutation::Renew(claim, lease) => {
            check_owner(&current, &actor, &claim, at)?;
            let end = expires(at, lease)?;
            let o = current.ownership_mut();
            o.heartbeat_at = Some(at);
            o.lease_expires_at = Some(o.lease_expires_at.unwrap().max(end));
            current.bump(at, TaskStatus::InProgress)?;
            LifecycleKind::Renewed
        }
        Mutation::Progress(input) => {
            check_owner(&current, &actor, &input.claim, at)?;
            super::canonical(&input.summary)?;
            super::canonical(&input.next_step)?;
            let end = expires(at, input.lease)?;
            let o = current.ownership_mut();
            o.heartbeat_at = Some(at);
            o.lease_expires_at = Some(o.lease_expires_at.unwrap().max(end));
            let p = current.progress_mut();
            let sequence = p
                .as_ref()
                .map_or(0, |p| p.observation.sequence)
                .checked_add(1)
                .ok_or(TaskStoreError::Invalid("progress sequence overflow"))?;
            *p = Some(Progress {
                observation: ProgressObservation {
                    kind: match target {
                        Target::Task(_) => ProgressKind::Progress,
                        Target::Session(_) => ProgressKind::Checkpoint,
                    },
                    sequence,
                    observed_at: at,
                },
                summary: input.summary,
                next_step: input.next_step,
            });
            current.bump(at, TaskStatus::InProgress)?;
            LifecycleKind::Progress
        }
        Mutation::Release(input) => {
            check_owner(&current, &actor, &input.claim, at)?;
            super::canonical(&input.summary)?;
            super::canonical(&input.next_step)?;
            let p = current.progress_mut();
            let sequence = p
                .as_ref()
                .map_or(0, |p| p.observation.sequence)
                .checked_add(1)
                .ok_or(TaskStoreError::Invalid("progress sequence overflow"))?;
            *p = Some(Progress {
                observation: ProgressObservation {
                    kind: match target {
                        Target::Task(_) => ProgressKind::Progress,
                        Target::Session(_) => ProgressKind::Checkpoint,
                    },
                    sequence,
                    observed_at: at,
                },
                summary: input.summary,
                next_step: input.next_step,
            });
            let epoch = current.ownership().claim_epoch;
            *current.ownership_mut() = Ownership {
                claim_epoch: epoch,
                ..Ownership::default()
            };
            current.set_fence(None);
            current.bump(at, TaskStatus::Open)?;
            LifecycleKind::Released
        }

        Mutation::SessionRevision(input) => {
            check_owner(&current, &actor, &input.claim, at)?;
            if current.status() != TaskStatus::InProgress {
                return Err(TaskStoreError::Invalid(
                    "specification requires in-progress session",
                ));
            }
            super::canonical(input.reason)?;
            for value in [input.title, input.description, input.strategy] {
                if let crate::task_spec::FieldUpdate::Set(value) = value {
                    super::canonical(value)?;
                }
            }
            let Current::Session(session) = &mut current else {
                return Err(TaskStoreError::Invalid("Session mutation target"));
            };
            let unchanged = match input.title {
                crate::task_spec::FieldUpdate::Keep => true,
                crate::task_spec::FieldUpdate::Set(value) => value == session.record.input.title,
            } && match input.description {
                crate::task_spec::FieldUpdate::Keep => true,
                crate::task_spec::FieldUpdate::Set(value) => {
                    value == session.record.input.description
                }
            } && match input.strategy {
                crate::task_spec::FieldUpdate::Keep => true,
                crate::task_spec::FieldUpdate::Set(value) => value == session.record.input.strategy,
            };
            let references_changed = match &references_update {
                references::TaskReferenceUpdate::Preserve => false,
                references::TaskReferenceUpdate::Replace(next) => {
                    next != &session.record.references
                }
            };
            if unchanged && !references_changed {
                return Err(TaskStoreError::Invalid("session specification unchanged"));
            }
            if let crate::task_spec::FieldUpdate::Set(value) = input.title {
                session.record.input.title = value.to_owned();
            }
            if let crate::task_spec::FieldUpdate::Set(value) = input.description {
                session.record.input.description = value.to_owned();
            }
            if let crate::task_spec::FieldUpdate::Set(value) = input.strategy {
                session.record.input.strategy = value.to_owned();
            }
            current.renew(at, input.lease)?;
            current.bump(at, TaskStatus::InProgress)?;
            LifecycleKind::Revised {
                reason: input.reason.to_owned(),
            }
        }
        Mutation::Specification(input) => {
            check_owner(&current, &actor, input.claim(), at)?;
            if current.status() != TaskStatus::InProgress {
                return Err(TaskStoreError::Invalid(
                    "specification requires in-progress task",
                ));
            }
            let lease = input.lease();
            let refs_changed = match &references_update {
                references::TaskReferenceUpdate::Preserve => false,
                references::TaskReferenceUpdate::Replace(next) => {
                    next != &current.task_mut()?.references
                }
            };
            let kind = specification::apply(
                original,
                current.task_mut()?,
                input,
                &actor,
                at,
                refs_changed,
            )?;
            current.renew(at, lease)?;
            kind
        }
        Mutation::Links(input) => {
            check_owner(&current, &actor, &input.claim, at)?;
            super::canonical(input.reason)?;
            let task = current.task_mut()?;
            let parent = match input.parent_id {
                crate::task_spec::FieldUpdate::Keep => task.parent_id,
                crate::task_spec::FieldUpdate::Set(p) => p,
            };
            let deps = input.depends_on.unwrap_or(&task.depends_on);
            if parent == task.parent_id && deps == task.depends_on {
                return Err(TaskStoreError::Invalid("links unchanged"));
            }
            task.parent_id = parent;
            task.depends_on = deps.to_vec();
            read::validate_relations(&capture.pin, task, original, limits.read, before).await?;
            current.renew(at, input.lease)?;
            current.bump(at, TaskStatus::InProgress)?;
            LifecycleKind::LinksUpdated {
                reason: input.reason.to_owned(),
            }
        }
        Mutation::Flag(input) => {
            check_owner(&current, &actor, &input.claim, at)?;
            super::canonical(input.reason)?;
            let task = current.task_mut()?;
            if task.flagged == input.flagged {
                return Err(TaskStoreError::Invalid("flag unchanged"));
            }
            task.flagged = input.flagged;
            current.renew(at, input.lease)?;
            current.bump(at, TaskStatus::InProgress)?;
            LifecycleKind::FlagUpdated {
                flagged: input.flagged,
                reason: input.reason.to_owned(),
            }
        }
        Mutation::Complete(claim) => {
            check_owner(&current, &actor, &claim, at)?;
            if current.status() != TaskStatus::InProgress {
                return Err(TaskStoreError::Invalid(
                    "completion requires in-progress record",
                ));
            }
            if let Current::Task(t) = &current
                && t.record.flagged
            {
                return Err(TaskStoreError::Invalid("flagged task cannot be completed"));
            }
            let (task, session) = match target {
                Target::Task(id) => (Some(id), None),
                Target::Session(id) => (None, Some(id)),
            };
            read::require_completion(&capture.pin, task, session, original, limits.read, before)
                .await?;
            current.clear_owner();
            current.bump(at, TaskStatus::Completed)?;
            LifecycleKind::Completed
        }
        Mutation::Commit(input) => {
            if current.revision() != input.expected_revision {
                return Err(TaskStoreError::RevisionConflict);
            }
            super::canonical(input.reason)?;
            if let Some(commit) = input.commit {
                graphit_core::git_record_event::validate_commit_id(commit).map_err(|_| {
                    TaskStoreError::Invalid("expected a full lowercase Git commit object ID")
                })?;
            }
            if let Some(claim) = &input.claim {
                native::require_operation(admission, &[ProjectWriteOperation::OwnTask])?;
                if claim.expected_revision != input.expected_revision {
                    return Err(TaskStoreError::RevisionConflict);
                }
                check_owner(&current, &actor, claim, at)?;
            } else {
                native::require_operation(admission, &[ProjectWriteOperation::Task])?;
                if current.ownership().live(at) {
                    return Err(TaskStoreError::FenceMismatch);
                }
            }
            if current.task_mut()?.commit.as_deref() == input.commit {
                before()?;
                return Ok((current.output(), None));
            }
            current.task_mut()?.commit = input.commit.map(str::to_owned);
            let status = current.status();
            current.bump(at, status)?;
            LifecycleKind::CommitUpdated {
                reason: input.reason.to_owned(),
            }
        }
        Mutation::Cancel(input) => {
            super::canonical(input.reason)?;
            match current.status() {
                TaskStatus::Open => {
                    native::require_operation(admission, &[ProjectWriteOperation::Task])?;
                    if input.claim.is_some() {
                        return Err(TaskStoreError::Invalid("open task has no live claim"));
                    }
                }
                TaskStatus::InProgress => {
                    let claim = input.claim.as_ref().ok_or(TaskStoreError::FenceMismatch)?;
                    check_owner(&current, &actor, claim, at)?;
                }
                _ => return Err(TaskStoreError::Invalid("terminal task cannot be cancelled")),
            }
            current.clear_owner();
            current.bump(at, TaskStatus::Cancelled)?;
            LifecycleKind::Cancelled {
                reason: input.reason.to_owned(),
            }
        }
    };
    match &mut current {
        Current::Task(t) => {
            references_update.apply(&mut t.record.references);
            t.record.reference_generation = t.record.specification.revision;
        }
        Current::Session(s) => {
            references_update.apply(&mut s.record.references);
            s.record.reference_generation = s.record.revision;
        }
    }
    let output = if let Some(comment) = &added_comment {
        Output::Comment(Box::new(TaskCommentOutcome::Created {
            task: current.task_mut()?.clone(),
            comment: comment.clone(),
        }))
    } else {
        current.output()
    };
    let token = current.fence().map(ClaimToken);
    let mut rows = current.publication(
        actor,
        kind,
        at,
        references_update.clone(),
        project_id,
        &project_version,
    );
    if let Some(comment) = added_comment {
        rows.push(rows::Row::TaskComment(rows::StoredComment {
            record: comment,
            references_update,
        }));
    }
    let capture = if defer_materialization {
        write::materialize(capture, original, admission, owner_lease_expires_at).await?
    } else {
        capture
    };
    write::commit(capture, original, admission, rows, owner_lease_expires_at).await?;
    Ok((output, token))
}
impl NativeTaskStore {
    pub async fn set_task_commit(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        input: TaskCommitUpdate<'_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Task, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Commit(input),
            crate::reference_projection::AuthoredReferencesField::Omitted,
            limits,
        )
        .await?
        .0
        .task())
    }

    pub async fn add_task_comment(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        input: TaskCommentUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<TaskCommentOutcome, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Comment(input),
            authored,
            limits,
        )
        .await?
        .0
        .comment())
    }
    pub async fn revise_task(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        input: RevisionUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Task, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Specification(specification::Mutation::Revise(input)),
            authored,
            limits,
        )
        .await?
        .0
        .task())
    }
    pub async fn verify_task_check(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        input: CheckUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Task, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Specification(specification::Mutation::Verify(input)),
            authored,
            limits,
        )
        .await?
        .0
        .task())
    }
    pub async fn supersede_task_check(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        input: SupersessionUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Task, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Specification(specification::Mutation::Supersede(input)),
            authored,
            limits,
        )
        .await?
        .0
        .task())
    }
    pub async fn update_task_links(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        input: TaskLinksUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Task, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Links(input),
            authored,
            limits,
        )
        .await?
        .0
        .task())
    }
    pub async fn set_task_flag(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        input: TaskFlagUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Task, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Flag(input),
            authored,
            limits,
        )
        .await?
        .0
        .task())
    }
    pub async fn complete_task(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        input: ClaimUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Task, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Complete(input),
            authored,
            limits,
        )
        .await?
        .0
        .task())
    }
    pub async fn cancel_task(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        input: TaskCancellation<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Task, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Cancel(input),
            authored,
            limits,
        )
        .await?
        .0
        .task())
    }
    pub async fn complete_session(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: SessionId,
        input: ClaimUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Session, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Session(id),
            Mutation::Complete(input),
            authored,
            limits,
        )
        .await?
        .0
        .session())
    }

    pub async fn claim_task(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        lease: Duration,
        limits: TaskStoreWriteLimits,
    ) -> Result<Claim<Task>, TaskStoreError> {
        let (record, token) = mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Claim(lease),
            crate::reference_projection::AuthoredReferencesField::Omitted,
            limits,
        )
        .await?;
        Ok(Claim {
            record: record.task(),
            token: token.unwrap(),
        })
    }
    pub async fn claim_session(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: SessionId,
        lease: Duration,
        limits: TaskStoreWriteLimits,
    ) -> Result<Claim<Session>, TaskStoreError> {
        let (record, token) = mutate(
            original,
            admission,
            Target::Session(id),
            Mutation::Claim(lease),
            crate::reference_projection::AuthoredReferencesField::Omitted,
            limits,
        )
        .await?;
        Ok(Claim {
            record: record.session(),
            token: token.unwrap(),
        })
    }
    pub async fn heartbeat_task(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        claim: ClaimUpdate<'_>,
        lease: Duration,
        limits: TaskStoreWriteLimits,
    ) -> Result<Task, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Renew(claim, lease),
            crate::reference_projection::AuthoredReferencesField::Omitted,
            limits,
        )
        .await?
        .0
        .task())
    }
    pub async fn heartbeat_session(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: SessionId,
        claim: ClaimUpdate<'_>,
        lease: Duration,
        limits: TaskStoreWriteLimits,
    ) -> Result<Session, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Session(id),
            Mutation::Renew(claim, lease),
            crate::reference_projection::AuthoredReferencesField::Omitted,
            limits,
        )
        .await?
        .0
        .session())
    }
    pub async fn progress_task(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        input: ProgressUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Task, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Progress(input),
            authored,
            limits,
        )
        .await?
        .0
        .task())
    }
    pub async fn revise_session(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: SessionId,
        input: SessionRevisionUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Session, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Session(id),
            Mutation::SessionRevision(input),
            authored,
            limits,
        )
        .await?
        .0
        .session())
    }
    pub async fn checkpoint_session(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: SessionId,
        input: ProgressUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Session, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Session(id),
            Mutation::Progress(input),
            authored,
            limits,
        )
        .await?
        .0
        .session())
    }
    pub async fn release_task(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: TaskId,
        input: ReleaseUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Task, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Task(id),
            Mutation::Release(input),
            authored,
            limits,
        )
        .await?
        .0
        .task())
    }
    pub async fn release_session(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        id: SessionId,
        input: ReleaseUpdate<'_>,
        authored: crate::reference_projection::AuthoredReferencesField<'_, '_>,
        limits: TaskStoreWriteLimits,
    ) -> Result<Session, TaskStoreError> {
        Ok(mutate(
            original,
            admission,
            Target::Session(id),
            Mutation::Release(input),
            authored,
            limits,
        )
        .await?
        .0
        .session())
    }
}
