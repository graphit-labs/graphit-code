use super::{TaskStoreError, model::*, write};
use crate::task_spec::{self, CheckId, NewCheck, ReplacementCheck, TaskSpecAuditKind};
use graphit_core::file_io::SnapshotContext;

pub(super) enum Mutation<'a> {
    Revise(RevisionUpdate<'a>),
    Verify(CheckUpdate<'a>),
    Supersede(SupersessionUpdate<'a>),
}
impl Mutation<'_> {
    pub(super) fn claim(&self) -> &ClaimUpdate<'_> {
        match self {
            Self::Revise(i) => &i.claim,
            Self::Verify(i) => &i.claim,
            Self::Supersede(i) => &i.claim,
        }
    }
    pub(super) fn lease(&self) -> std::time::Duration {
        match self {
            Self::Revise(i) => i.lease,
            Self::Verify(i) => i.lease,
            Self::Supersede(i) => i.lease,
        }
    }
}
pub(super) fn apply(
    original: &SnapshotContext,
    task: &mut Task,
    input: Mutation<'_>,
    actor: &str,
    at: u64,
    references_changed: bool,
) -> Result<LifecycleKind, TaskStoreError> {
    let delta = match input {
        Mutation::Revise(i) => {
            let unchanged = match i.title {
                crate::task_spec::FieldUpdate::Keep => true,
                crate::task_spec::FieldUpdate::Set(v) => v == task.specification.title,
            } && match i.description {
                crate::task_spec::FieldUpdate::Keep => true,
                crate::task_spec::FieldUpdate::Set(v) => v == task.specification.description,
            } && match i.task_type {
                crate::task_spec::FieldUpdate::Keep => true,
                crate::task_spec::FieldUpdate::Set(v) => v == task.specification.task_type,
            } && i.priority.is_none_or(|v| v == task.specification.priority)
                && match i.schedule {
                    crate::task_spec::FieldUpdate::Keep => true,
                    crate::task_spec::FieldUpdate::Set(v) => v == task.specification.schedule,
                }
                && i.add_checks.is_empty();
            if references_changed && unchanged {
                original.check()?;
                super::canonical(i.reason)?;
                if task.specification.revision != i.claim.expected_revision {
                    return Err(TaskStoreError::RevisionConflict);
                }
                if at < task.updated_at {
                    return Err(TaskStoreError::Invalid("clock moved backwards"));
                }
                task.specification.revision = task
                    .specification
                    .revision
                    .checked_add(1)
                    .ok_or(TaskStoreError::Invalid("revision overflow"))?;
                task.updated_at = at;
                return Ok(LifecycleKind::Revised {
                    reason: i.reason.to_owned(),
                });
            }
            let additions = i
                .add_checks
                .iter()
                .map(|c| {
                    Ok(NewCheck {
                        id: CheckId::new(write::fresh()?),
                        kind: c.kind,
                        text: &c.text,
                    })
                })
                .collect::<Result<Vec<_>, TaskStoreError>>()?;
            task_spec::plan_revision(
                &task.specification,
                task_spec::RevisionCommand {
                    expected_revision: i.claim.expected_revision,
                    title: i.title,
                    description: i.description,
                    task_type: i.task_type,
                    priority: i.priority,
                    schedule: i.schedule,
                    add_checks: &additions,
                    actor,
                    reason: i.reason,
                    occurred_at: at,
                },
                original,
            )?
        }
        Mutation::Verify(i) => task_spec::plan_check_verification(
            &task.specification,
            task_spec::CheckVerificationCommand {
                expected_revision: i.claim.expected_revision,
                check_id: i.check_id,
                passed: i.passed,
                evidence: i.evidence,
                actor,
                occurred_at: at,
            },
            original,
        )?,
        Mutation::Supersede(i) => {
            let replacement = i
                .replacement
                .map(|r| {
                    Ok::<_, TaskStoreError>(ReplacementCheck {
                        id: CheckId::new(write::fresh()?),
                        text: r.text,
                        kind: r.kind,
                    })
                })
                .transpose()?;
            task_spec::plan_check_supersession(
                &task.specification,
                task_spec::CheckSupersessionCommand {
                    expected_revision: i.claim.expected_revision,
                    check_id: i.check_id,
                    actor,
                    reason: i.reason,
                    occurred_at: at,
                    replacement,
                },
                original,
            )?
        }
    };
    let (_, after, audit) = delta.into_parts();
    let kind = match audit.kind {
        TaskSpecAuditKind::Revision => LifecycleKind::Revised {
            reason: audit.reason,
        },
        TaskSpecAuditKind::CheckVerified { check_id } => LifecycleKind::CheckVerified { check_id },
        TaskSpecAuditKind::CheckSuperseded {
            check_id,
            replacement,
        } => LifecycleKind::CheckSuperseded {
            check_id,
            replacement,
            reason: audit.reason,
        },
    };
    task.specification = after;
    task.updated_at = at;
    Ok(kind)
}
