use super::*;
use crate::{task_spec::FieldUpdate, task_store::*};
use graphit_auth::ProjectWriteAdmission;
use graphit_core::file_io::SnapshotContext;
use std::sync::Arc;

fn text(input: &FieldUpdate<String>) -> FieldUpdate<&str> {
    match input {
        FieldUpdate::Keep => FieldUpdate::Keep,
        FieldUpdate::Set(value) => FieldUpdate::Set(value.as_str()),
    }
}
/// Dispatches classified native writes without recreating authority or retrying a stale decision.
pub async fn execute_write(
    mut request: WriteRequest,
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    limits: TaskStoreWriteLimits,
) -> Result<WriteResponse, TaskStoreError> {
    original.check()?;
    if !Arc::ptr_eq(original, admission.request()?.snapshot()) {
        return Err(TaskStoreError::DifferentRequest);
    }
    admission.recheck()?;
    if admission.operation() != request.required_operation() {
        return Err(TaskStoreError::WrongAdmission);
    }
    let references = request.take_references();
    let entries = reference_entries(&references, original, limits.references)?;
    let authored = match &references {
        FieldUpdate::Keep => crate::reference_projection::AuthoredReferencesField::Omitted,
        FieldUpdate::Set(serde_json::Value::Null) => {
            crate::reference_projection::AuthoredReferencesField::Null
        }
        FieldUpdate::Set(serde_json::Value::Array(_)) => {
            crate::reference_projection::AuthoredReferencesField::Sequence(&entries)
        }
        FieldUpdate::Set(_) => crate::reference_projection::AuthoredReferencesField::NonSequence,
    };
    use WriteResponse::{Session, SessionClaim, Task, TaskClaim};
    Ok(match request {
        WriteRequest::SetTaskCommit(input) => {
            let FieldUpdate::Set(commit) = &input.commit else {
                return Err(TaskStoreError::Invalid(
                    "commit field is required; null clears it",
                ));
            };
            Task(
                Box::pin(NativeTaskStore::set_task_commit(
                    original,
                    admission,
                    input.id,
                    TaskCommitUpdate {
                        expected_revision: input.expected_revision,
                        claim: input.claim.as_ref().map(Fence::update),
                        commit: commit.as_deref(),
                        reason: &input.reason,
                    },
                    limits,
                ))
                .await?,
            )
        }
        WriteRequest::AddTaskComment(input) => WriteResponse::Comment(Box::new(
            Box::pin(NativeTaskStore::add_task_comment(
                original,
                admission,
                input.target.id,
                TaskCommentUpdate {
                    claim: input.target.claim.update(),
                    lease: input.lease_ms.duration(),
                    idempotency_key: input.idempotency_key,
                    kind: input.kind,
                    body: input.body,
                },
                authored,
                limits,
            ))
            .await?,
        )),
        WriteRequest::CreateTask(input) => Task(
            Box::pin(NativeTaskStore::create_task(
                original,
                admission,
                input.into_native(),
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::CreateSession(input) => Session(
            Box::pin(NativeTaskStore::create_session(
                original,
                admission,
                input.into_native(),
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::ClaimTask(input) => TaskClaim(
            Box::pin(NativeTaskStore::claim_task(
                original,
                admission,
                input.id,
                input.lease_ms.duration(),
                limits,
            ))
            .await?
            .into(),
        ),
        WriteRequest::ClaimSession(input) => SessionClaim(
            Box::pin(NativeTaskStore::claim_session(
                original,
                admission,
                input.id,
                input.lease_ms.duration(),
                limits,
            ))
            .await?
            .into(),
        ),
        WriteRequest::HeartbeatTask(input) => Task(
            Box::pin(NativeTaskStore::heartbeat_task(
                original,
                admission,
                input.target.id,
                input.target.claim.update(),
                input.lease_ms.duration(),
                limits,
            ))
            .await?,
        ),
        WriteRequest::HeartbeatSession(input) => Session(
            Box::pin(NativeTaskStore::heartbeat_session(
                original,
                admission,
                input.target.id,
                input.target.claim.update(),
                input.lease_ms.duration(),
                limits,
            ))
            .await?,
        ),
        WriteRequest::ProgressTask(input) => Task(
            Box::pin(NativeTaskStore::progress_task(
                original,
                admission,
                input.target.id,
                ProgressUpdate {
                    claim: input.target.claim.update(),
                    lease: input.lease_ms.duration(),
                    summary: input.summary,
                    next_step: input.next_step,
                },
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::CheckpointSession(input) => Session(
            Box::pin(NativeTaskStore::checkpoint_session(
                original,
                admission,
                input.target.id,
                ProgressUpdate {
                    claim: input.target.claim.update(),
                    lease: input.lease_ms.duration(),
                    summary: input.summary,
                    next_step: input.next_step,
                },
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::ReleaseTask(input) => Task(
            Box::pin(NativeTaskStore::release_task(
                original,
                admission,
                input.target.id,
                ReleaseUpdate {
                    claim: input.target.claim.update(),
                    summary: input.summary,
                    next_step: input.next_step,
                },
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::ReleaseSession(input) => Session(
            Box::pin(NativeTaskStore::release_session(
                original,
                admission,
                input.target.id,
                ReleaseUpdate {
                    claim: input.target.claim.update(),
                    summary: input.summary,
                    next_step: input.next_step,
                },
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::ReviseTask(input) => Task(
            Box::pin(NativeTaskStore::revise_task(
                original,
                admission,
                input.target.id,
                RevisionUpdate {
                    claim: input.target.claim.update(),
                    lease: input.lease_ms.duration(),
                    reason: &input.reason,
                    title: text(&input.title),
                    description: text(&input.description),
                    task_type: text(&input.task_type),
                    priority: input.priority,
                    schedule: input.schedule,
                    add_checks: &input.add_checks,
                },
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::ReviseSession(input) => Session(
            Box::pin(NativeTaskStore::revise_session(
                original,
                admission,
                input.target.id,
                SessionRevisionUpdate {
                    claim: input.target.claim.update(),
                    lease: input.lease_ms.duration(),
                    reason: &input.reason,
                    title: text(&input.title),
                    description: text(&input.description),
                    strategy: text(&input.strategy),
                },
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::VerifyTaskCheck(input) => Task(
            Box::pin(NativeTaskStore::verify_task_check(
                original,
                admission,
                input.target.id,
                CheckUpdate {
                    claim: input.target.claim.update(),
                    lease: input.lease_ms.duration(),
                    check_id: input.check_id,
                    passed: input.passed,
                    evidence: &input.evidence,
                },
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::SupersedeTaskCheck(input) => Task(
            Box::pin(NativeTaskStore::supersede_task_check(
                original,
                admission,
                input.target.id,
                SupersessionUpdate {
                    claim: input.target.claim.update(),
                    lease: input.lease_ms.duration(),
                    check_id: input.check_id,
                    reason: &input.reason,
                    replacement: input.replacement.as_ref().map(|value| ReplacementInput {
                        text: &value.text,
                        kind: value.kind,
                    }),
                },
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::UpdateTaskLinks(input) => Task(
            Box::pin(NativeTaskStore::update_task_links(
                original,
                admission,
                input.target.id,
                TaskLinksUpdate {
                    claim: input.target.claim.update(),
                    lease: input.lease_ms.duration(),
                    parent_id: input.parent_id,
                    depends_on: input.depends_on.as_deref(),
                    reason: &input.reason,
                },
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::SetTaskFlag(input) => Task(
            Box::pin(NativeTaskStore::set_task_flag(
                original,
                admission,
                input.target.id,
                TaskFlagUpdate {
                    claim: input.target.claim.update(),
                    lease: input.lease_ms.duration(),
                    flagged: input.flagged,
                    reason: &input.reason,
                },
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::CompleteTask(input) => Task(
            Box::pin(NativeTaskStore::complete_task(
                original,
                admission,
                input.id,
                input.claim.update(),
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::CancelTask(input) => Task(
            Box::pin(NativeTaskStore::cancel_task(
                original,
                admission,
                input.id,
                TaskCancellation {
                    claim: input.claim.as_ref().map(Fence::update),
                    reason: &input.reason,
                },
                authored,
                limits,
            ))
            .await?,
        ),
        WriteRequest::CompleteSession(input) => Session(
            Box::pin(NativeTaskStore::complete_session(
                original,
                admission,
                input.id,
                input.claim.update(),
                authored,
                limits,
            ))
            .await?,
        ),
    })
}

fn reference_entries<'a>(
    field: &'a FieldUpdate<serde_json::Value>,
    original: &SnapshotContext,
    limits: TaskReferencesInputLimits,
) -> Result<Vec<crate::reference_projection::AuthoredReferenceEntry<'a>>, TaskStoreError> {
    use crate::reference_projection::{AuthoredReferenceEntry as Entry, ReferenceCandidate};
    original.check()?;
    let FieldUpdate::Set(serde_json::Value::Array(values)) = field else {
        return Ok(Vec::new());
    };
    if values.len() > limits.max_entries.get() {
        return Err(TaskStoreError::Invalid("reference input entry limit"));
    }
    let mut bytes = 0usize;
    let mut entries = Vec::new();
    for value in values {
        original.check()?;
        let candidate = (|| {
            let entry = value.as_object()?;
            if entry
                .keys()
                .any(|k| !matches!(k.as_str(), "target" | "relation" | "field"))
            {
                return None;
            }
            let target = entry.get("target")?.as_object()?;
            if target
                .keys()
                .any(|k| !matches!(k.as_str(), "type" | "id" | "scope" | "scope_id" | "context"))
            {
                return None;
            }
            let optional = |map: &'a serde_json::Map<String, serde_json::Value>,
                            key|
             -> Option<Option<&'a str>> {
                match map.get(key) {
                    None => Some(None),
                    Some(v) => v.as_str().map(Some),
                }
            };
            Some(ReferenceCandidate {
                target_type: target.get("type")?.as_str()?,
                id: target.get("id")?.as_str()?,
                scope: optional(target, "scope")?,
                scope_id: optional(target, "scope_id")?,
                context: optional(target, "context")?,
                relation: entry.get("relation")?.as_str()?,
                field: optional(entry, "field")?,
            })
        })();
        if let Some(c) = candidate {
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
            entries.push(Entry::Tuple(c));
        } else {
            entries.push(Entry::InvalidShape);
        }
    }
    original.check()?;
    Ok(entries)
}
