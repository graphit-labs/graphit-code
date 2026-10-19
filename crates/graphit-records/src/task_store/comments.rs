use super::{TaskStoreError, model::*};
use crate::gates::TaskStatus;
use std::collections::HashSet;

pub(super) fn assemble(
    task: &Task,
    events: &[LifecycleEvent<Task>],
    comments: &mut [super::rows::StoredComment],
) -> Result<(), TaskStoreError> {
    comments.sort_unstable_by_key(|c| c.sequence);
    let mut ids = HashSet::new();
    let mut keys = HashSet::new();
    let mut revision = 1;
    let mut at = task.created_at;
    for (index, comment) in comments.iter().enumerate() {
        comment.validate().map_err(TaskStoreError::Invalid)?;
        let sequence = u64::try_from(index)
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(TaskStoreError::Invalid("comment sequence overflow"))?;
        if comment.task_id != task.specification.id
            || comment.session_id != task.session_id
            || comment.sequence.get() != sequence
            || !ids.insert(comment.id)
            || !keys.insert(comment.idempotency_key.as_str())
            || comment.revision <= revision
            || comment.at < at
            || comment.at > task.updated_at
        {
            return Err(TaskStoreError::Invalid("comment history"));
        }
        let event = event(events, comment)?;
        validate_binding(event, comment)?;
        revision = comment.revision;
        at = comment.at;
    }
    if events
        .iter()
        .filter(|e| matches!(e.kind, LifecycleKind::CommentAdded { .. }))
        .count()
        != comments.len()
    {
        return Err(TaskStoreError::Invalid("missing comment"));
    }
    Ok(())
}
pub(super) fn validate_binding(
    event: &LifecycleEvent<Task>,
    comment: &super::rows::StoredComment,
) -> Result<(), TaskStoreError> {
    comment.validate().map_err(TaskStoreError::Invalid)?;
    self::event(std::slice::from_ref(event), comment)?;
    if event.references_update != comment.references_update {
        return Err(TaskStoreError::Invalid(
            "comment reference request identity",
        ));
    }
    if event.record.specification.id != comment.task_id
        || event.record.session_id != comment.session_id
        || event.actor != comment.actor
        || event.observed_at != comment.at
        || event.record.updated_at != comment.at
        || event.record.status != TaskStatus::InProgress
        || event.record.ownership.owner.as_deref() != Some(comment.actor.as_str())
        || event.record.ownership.claim_epoch == 0
        || !event.record.ownership.live(comment.at)
    {
        return Err(TaskStoreError::Invalid("comment event attribution"));
    }
    Ok(())
}
pub(super) fn event<'a>(
    events: &'a [LifecycleEvent<Task>],
    comment: &TaskComment,
) -> Result<&'a LifecycleEvent<Task>, TaskStoreError> {
    let index = events
        .binary_search_by_key(&comment.revision, |e| e.record.specification.revision)
        .map_err(|_| TaskStoreError::Invalid("missing comment event"))?;
    let event = &events[index];
    if !matches!(event.kind, LifecycleKind::CommentAdded { id, sequence } if id == comment.id && sequence == comment.sequence)
    {
        return Err(TaskStoreError::Invalid("comment event binding"));
    }
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_spec::{TaskId, TaskSpecification};
    use std::num::{NonZeroU64, NonZeroU128};
    fn nz(n: u128) -> NonZeroU128 {
        NonZeroU128::new(n).unwrap()
    }
    fn fixture() -> (
        Task,
        Vec<LifecycleEvent<Task>>,
        Vec<super::super::rows::StoredComment>,
    ) {
        let actor = "a".repeat(64);
        let mut task = Task {
            commit: None,
            specification: TaskSpecification {
                id: TaskId::new(nz(1)),
                revision: 4,
                title: "Task".into(),
                description: "Behavior".into(),
                task_type: "feature".into(),
                priority: 2,
                schedule: None,
                checks: vec![],
            },
            session_id: SessionId::new(nz(2)),
            parent_id: None,
            depends_on: vec![],
            references: vec![],
            reference_generation: 4,
            idempotency_key: "task".into(),
            status: TaskStatus::InProgress,
            flagged: false,
            progress: None,
            created_at: 10,
            updated_at: 20,
            ownership: Ownership {
                owner: Some(actor.clone()),
                claim_epoch: 1,
                claimed_at: Some(10),
                heartbeat_at: Some(20),
                lease_expires_at: Some(30),
            },
        };
        let comments: Vec<_> = (1..=2)
            .map(|n| TaskComment {
                id: TaskCommentId::new(nz(n + 2)),
                task_id: task.specification.id,
                session_id: task.session_id,
                idempotency_key: format!("key{n}"),
                sequence: NonZeroU64::new(n as u64).unwrap(),
                kind: TaskCommentKind::Note,
                body: "  literal\n".into(),
                actor: actor.clone(),
                at: 20,
                revision: n as u64 + 2,
            })
            .collect();
        let events = comments
            .iter()
            .map(|c| {
                task.specification.revision = c.revision;
                task.reference_generation = c.revision;
                LifecycleEvent {
                    actor: actor.clone(),
                    observed_at: 20,
                    kind: LifecycleKind::CommentAdded {
                        id: c.id,
                        sequence: c.sequence,
                    },
                    record: task.clone(),
                    references_update: super::super::references::TaskReferenceUpdate::Preserve,
                }
            })
            .collect();
        let comments = comments
            .into_iter()
            .map(|record| super::super::rows::StoredComment {
                record,
                references_update: super::super::references::TaskReferenceUpdate::Preserve,
            })
            .collect();
        (task, events, comments)
    }
    #[test]
    fn comments_equal_milliseconds_are_legal_but_binding_and_history_are_strict() {
        let (task, events, mut comments) = fixture();
        comments.reverse();
        assemble(&task, &events, &mut comments).unwrap();
        assert_eq!(comments[0].sequence.get(), 1);
        for case in 0..9 {
            let mut bad = comments.clone();
            match case {
                0 => bad[1].sequence = bad[0].sequence,
                1 => bad[1].id = bad[0].id,
                2 => bad[1].idempotency_key = bad[0].idempotency_key.clone(),
                3 => bad[1].revision = bad[0].revision,
                4 => bad[1].at = 19,
                5 => bad[1].at = 21,
                6 => bad[1].task_id = TaskId::new(nz(77)),
                7 => bad[1].session_id = SessionId::new(nz(77)),
                _ => bad[1].body = " \n ".into(),
            }
            assert!(assemble(&task, &events, &mut bad).is_err(), "case {case}");
        }
        for case in 0..9 {
            let mut bad = events.clone();
            match case {
                0 => bad[1].record.session_id = SessionId::new(nz(77)),
                1 => bad[1].record.specification.id = TaskId::new(nz(77)),
                2 => bad[1].actor = "b".repeat(64),
                3 => bad[1].observed_at = 19,
                4 => bad[1].record.updated_at = 19,
                5 => bad[1].record.status = TaskStatus::Open,
                6 => bad[1].record.ownership.claim_epoch = 0,
                7 => bad[1].record.ownership.lease_expires_at = Some(20),
                _ => bad[1].kind = LifecycleKind::Renewed,
            }
            assert!(
                assemble(&task, &bad, &mut comments.clone()).is_err(),
                "event case {case}"
            );
        }
        assert!(assemble(&task, &events[..1], &mut comments.clone()).is_err());
        assert!(assemble(&task, &events, &mut comments[..1]).is_err());
    }
}
