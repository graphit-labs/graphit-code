//! Runtime-only Git events re-capture Git facts and both ordinary write grants.
use super::*;
use graphit_auth::ProjectWriteOperation;
use graphit_core::{
    git_record_event::{GitRecordEvent, GitRecordFacts},
    version_context::Checkout,
};
use graphit_records::{
    git_records::{GitRecordsTransition, apply},
    memory_store::{MemoryStoreReadLimits, MemoryStoreWriteLimits},
};
use std::num::NonZeroUsize;

pub(super) async fn http(
    AxumState(server): AxumState<TaskServer>,
    headers: HeaderMap,
    Json(input): Json<crate::git_records::Request>,
) -> HttpResult {
    if headers.contains_key(axum::http::header::ORIGIN) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error":{"code":"runtime_required"}})),
        ));
    }
    if headers
        .get_all(axum::http::header::AUTHORIZATION)
        .iter()
        .count()
        != 1
        || headers.get_all("x-graphit-auth-origin").iter().count() != 1
    {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":{"code":"credential_required"}})),
        ));
    }
    let credential = Credential::read(&headers, &server.0.policy).map_err(|code| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":{"code":code}})),
        )
    })?;
    if !matches!(credential.origin, Origin::Runtime) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error":{"code":"runtime_required"}})),
        ));
    }
    let state = Arc::clone(&server.0);
    let cancellation = Cancellation::default();
    let _guard = CancelOnDrop(cancellation.clone());
    let deadline = Instant::now() + state.policy.timeout;
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! {
            jobs = state.jobs.lock() => jobs,
            () = state.stop.cancelled() => return Err((StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":{"code":"stopped"}})))),
            () = tokio::time::sleep_until(deadline.into()) => return Err((StatusCode::GATEWAY_TIMEOUT, Json(json!({"error":{"code":"deadline"}})))),
        };
        while jobs.try_join_next().is_some() {}
        if state.stop.is_cancelled() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error":{"code":"stopped"}})),
            ));
        }
        let owner = Arc::clone(&state);
        jobs.spawn(async move {
            let result = match RequestContext::new(owner.policy.content, &cancellation, deadline) {
                Ok(original) => execute(&owner, input, credential, &original).await,
                Err(_) => Err(io::ErrorKind::TimedOut.into()),
            };
            let _ = send.send(result);
        });
    }
    let result = tokio::select! {
        value = receive => value.unwrap_or_else(|_| Err(io::ErrorKind::BrokenPipe.into())),
        () = state.stop.cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(deadline.into()) => Err(io::ErrorKind::TimedOut.into()),
    };
    match result {
        Ok(value)
            if value.get("state").and_then(Value::as_str) == Some("applied")
                || value.get("state").and_then(Value::as_str) == Some("non_git")
                || value.get("state").and_then(Value::as_str) == Some("unborn") =>
        {
            Ok(Json(value))
        }
        Ok(value)
            if matches!(
                value.get("state").and_then(Value::as_str),
                Some("ignored" | "pending_snapshot")
            ) =>
        {
            Ok(Json(value))
        }
        Ok(value) => {
            let status = match value.get("code").and_then(Value::as_str) {
                Some("access_denied") => StatusCode::FORBIDDEN,
                Some("request_cancelled_or_expired") => StatusCode::GATEWAY_TIMEOUT,
                Some("invalid_git_records_transition") => StatusCode::BAD_REQUEST,
                _ => StatusCode::CONFLICT,
            };
            Err((status, Json(value)))
        }
        Err(error) => Err({
            let (status, code) = match error.kind() {
                io::ErrorKind::PermissionDenied => {
                    (StatusCode::FORBIDDEN, "authentication_or_permission_denied")
                }
                io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => {
                    (StatusCode::BAD_REQUEST, "invalid_git_event")
                }
                io::ErrorKind::TimedOut | io::ErrorKind::Interrupted => {
                    (StatusCode::GATEWAY_TIMEOUT, "request_cancelled_or_expired")
                }
                _ => (StatusCode::CONFLICT, "git_records_failed"),
            };
            // Fixed transport error categories cannot leak storage URLs/credentials.
            (status, Json(json!({"error":{"code":code}})))
        }),
    }
}

async fn execute(
    state: &State,
    input: crate::git_records::Request,
    credential: Credential,
    original: &RequestContext<'_>,
) -> io::Result<Value> {
    original.check()?;
    let live = state.live_projects.snapshot()?;
    let (paths, id, _) = live
        .projects
        .iter()
        .find(|(paths, _, _)| paths.root() == input.project_dir.as_path())
        .ok_or(io::ErrorKind::PermissionDenied)?;
    let paths = paths.clone();
    let id = *id;
    let project =
        CapturedPublicationScope::project(&paths, id, state.policy.scope, original.snapshot())
            .map_err(io::Error::other)?;
    let binding = project.binding().map_err(io::Error::other)?;
    let issuer = state.auth.issuer();
    let caller = issuer
        .authenticate_runtime(&credential.bearer, original)
        .map_err(|_| io::Error::from(io::ErrorKind::PermissionDenied))?;
    let root = paths.root().to_owned();
    let snapshot = Arc::clone(original.snapshot());
    let facts = tokio::task::spawn_blocking(move || {
        let remaining = snapshot
            .required_deadline()?
            .checked_duration_since(Instant::now())
            .ok_or(io::ErrorKind::TimedOut)?;
        GitRecordFacts::inspect(&root, remaining, snapshot.cancellation())
    })
    .await
    .map_err(io::Error::other)??;
    let Some(facts) = facts else {
        return Ok(json!({"state":"non_git"}));
    };
    if facts.root != paths.root() {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let checkout = match (facts.branch.as_deref(), facts.head.as_deref()) {
        (Some(name), _) => Checkout::Branch {
            name,
            default: &facts.default_branch,
        },
        (None, Some(commit)) => Checkout::Detached {
            default: &facts.default_branch,
            commit,
        },
        _ => return Err(io::ErrorKind::InvalidData.into()),
    };
    let version = VersionContext::select(&paths, checkout, "", None, "")?;
    let scope = ScopeInput {
        installation: state.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &project,
                record: &binding,
            },
            version: &version,
        }),
        providers: &[],
        binding: state.policy.binding,
        global_version: "",
    };
    let authority = ProjectRecordAuthority::new(issuer);
    let task = authority
        .admit_authenticated_write(
            &caller,
            original,
            scope,
            ProjectWriteOperation::CoordinateTask,
        )
        .map_err(|_| io::Error::from(io::ErrorKind::PermissionDenied))?;
    let memory = authority
        .admit_authenticated_write(&caller, original, scope, ProjectWriteOperation::Memory)
        .map_err(|_| io::Error::from(io::ErrorKind::PermissionDenied))?;
    let Some(head) = facts.head.as_deref() else {
        return Ok(json!({"state":"unborn"}));
    };
    if let Some(GitRecordEvent::ReferenceTransaction { updates }) = &input.event {
        let reference = facts
            .branch
            .as_ref()
            .map(|branch| format!("refs/heads/{branch}"));
        if !updates.iter().any(|update| {
            update.new == head
                && (update.reference == "HEAD" || Some(&update.reference) == reference.as_ref())
        }) {
            return Ok(json!({"state":"ignored"}));
        }
    }
    let transitions = transitions(&facts, &input, original)?;
    let n = |value| NonZeroUsize::new(value).ok_or(io::ErrorKind::InvalidInput);
    let memory_limits = MemoryStoreWriteLimits {
        read: MemoryStoreReadLimits {
            max_query_bytes: n(state.policy.content.max_input_bytes)?,
            batch_rows: state.policy.read.batch_rows,
            native: state.policy.read.native,
        },
        artifacts: state.policy.write.artifacts,
        max_input_bytes: n(state.policy.content.max_input_bytes)?,
        max_row_bytes: n(state.policy.content.max_input_bytes)?,
    };
    if let Some(GitRecordEvent::Rewrite { pairs }) = &input.event {
        for pair in pairs {
            if !graphit_records::git_records::has_git_commit(
                original.snapshot(),
                &memory,
                &task,
                &pair.new,
                memory_limits,
                state.policy.write,
            )
            .await
            .map_err(io::Error::other)?
            {
                return Ok(
                    json!({"state":"blocked", "code":"git_rewrite_snapshot_missing", "commit":pair.new}),
                );
            }
        }
    }
    if matches!(
        &input.event,
        Some(GitRecordEvent::ReferenceTransaction { .. })
    ) && !graphit_records::git_records::has_git_commit(
        original.snapshot(),
        &memory,
        &task,
        head,
        memory_limits,
        state.policy.write,
    )
    .await
    .map_err(io::Error::other)?
    {
        // Another instance needs only the authoritative native tags. Creation
        // itself is paired by post-commit/post-applypatch/post-rewrite.
        return Ok(json!({"state":"pending_snapshot","commit":head}));
    }
    let mut results = Vec::new();
    for transition in transitions {
        original.check()?;
        let outcome = match apply(
            original.snapshot(),
            &memory,
            &task,
            transition,
            memory_limits,
            state.policy.write,
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                return Ok(json!({"state":"failed", "code":error.code(),
                    "commit":head,"results":results,"partial":error.partial,"recovery_key":error.recovery_key()}));
            }
        };
        let value = serde_json::to_value(&outcome)?;
        if matches!(
            outcome,
            graphit_records::git_records::GitRecordsOutcome::Blocked { .. }
        ) {
            return Ok(json!({"state":"blocked", "commit":head,"results":results,"detail":value}));
        }
        results.push(value);
    }
    TaskServer::wake_embeddings(state, &paths, id, &version);
    original.check()?;
    // Git may have moved again while IO was pending: don't report an unrelated
    // snapshot as equivalent to the current working tree.
    let now = GitRecordFacts::inspect(
        paths.root(),
        original
            .snapshot()
            .required_deadline()?
            .saturating_duration_since(Instant::now()),
        original.cancellation(),
    )?;
    if now.as_ref().is_none_or(|now| {
        now.head.as_deref() != Some(head)
            || now.branch != facts.branch
            || now.root != facts.root
            || now.common_dir != facts.common_dir
            || now.replayed_commit != facts.replayed_commit
    }) {
        return Ok(json!({"state":"head_changed","commit":head,"results":results}));
    }
    Ok(
        json!({"state":"applied","commit":head,"project_version":version.version(),"results":results}),
    )
}

fn transitions(
    facts: &GitRecordFacts,
    input: &crate::git_records::Request,
    original: &RequestContext<'_>,
) -> io::Result<Vec<GitRecordsTransition>> {
    let head = facts.head.as_ref().ok_or(io::ErrorKind::InvalidInput)?;
    let timeout = || {
        original
            .snapshot()
            .required_deadline()?
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
    };
    let mut operations = Vec::new();
    let snapshot = || GitRecordsTransition::Commit {
        commit: head.clone(),
    };
    if input.initialize {
        if input.event.is_some() || input.restore || input.restore_previous.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        return Ok(vec![GitRecordsTransition::Initialize {
            commit: head.clone(),
        }]);
    }
    let merge = |ours: &str, theirs: &str| -> io::Result<GitRecordsTransition> {
        let base = facts
            .merge_base(ours, theirs, timeout()?, original.cancellation())?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Git record merge has no common ancestor",
                )
            })?;
        Ok(GitRecordsTransition::Merge {
            base,
            theirs: theirs.to_owned(),
        })
    };
    if input.restore {
        if input.event.is_some() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        operations.push(GitRecordsTransition::Checkout {
            commit: head.clone(),
            previous_commit: input.restore_previous.clone(),
        });
        return Ok(operations);
    }
    if input.restore_previous.is_some() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    match &input.event {
        None => operations.push(snapshot()),
        Some(GitRecordEvent::Commit {}) => {
            if let Some(source) = &facts.replayed_commit {
                let parents = facts.parents_of(source, timeout()?, original.cancellation())?;
                let [base] = parents.as_slice() else {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "Record replay requires a single source parent",
                    ));
                };
                let [parent] = facts.parents.as_slice() else {
                    return Err(io::ErrorKind::InvalidData.into());
                };
                if facts.branch.is_none() {
                    operations.push(GitRecordsTransition::Checkout {
                        commit: parent.clone(),
                        previous_commit: None,
                    });
                }
                operations.push(GitRecordsTransition::Merge {
                    base: base.clone(),
                    theirs: source.clone(),
                });
            } else {
                match facts.parents.as_slice() {
                    [ours, theirs] => operations.push(merge(ours, theirs)?),
                    parents if parents.len() > 2 => {
                        return Err(io::Error::new(
                            io::ErrorKind::Unsupported,
                            "Octopus record merge requires explicit reconciliation",
                        ));
                    }
                    _ => {}
                }
            }
            operations.push(snapshot());
        }
        Some(GitRecordEvent::Checkout { old, new, branch }) => {
            if new != head {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if *branch {
                operations.push(GitRecordsTransition::Checkout {
                    commit: new.clone(),
                    previous_commit: (!old.bytes().all(|byte| byte == b'0')).then(|| old.clone()),
                });
            }
        }
        Some(GitRecordEvent::Merge { squash }) => {
            if *squash {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Squash record merge must be completed by its commit",
                ));
            }
            let previous = facts
                .previous_head(timeout()?, original.cancellation())?
                .ok_or(io::ErrorKind::InvalidData)?;
            // A normal merge was already processed by post-commit. Repeating
            // the same three-way operation is idempotent in the record layer.
            let theirs = if facts.parents.len() == 2 {
                &facts.parents[1]
            } else {
                head
            };
            operations.push(merge(&previous, theirs)?);
            operations.push(snapshot());
        }
        Some(GitRecordEvent::Rewrite { pairs }) => {
            for pair in pairs {
                facts.resolve_commit(&pair.old, timeout()?, original.cancellation())?;
                facts.resolve_commit(&pair.new, timeout()?, original.cancellation())?;
            }
            let last = pairs.last().ok_or(io::ErrorKind::InvalidInput)?;
            if &last.new != head {
                return Err(io::ErrorKind::InvalidData.into());
            }
            operations.push(GitRecordsTransition::Checkout {
                commit: head.clone(),
                previous_commit: Some(last.old.clone()),
            });
        }
        Some(GitRecordEvent::ReferenceTransaction { updates }) => {
            let reference = facts
                .branch
                .as_ref()
                .map(|branch| format!("refs/heads/{branch}"));
            if let Some(update) = updates.iter().find(|update| {
                update.new == *head
                    && (update.reference == "HEAD" || Some(&update.reference) == reference.as_ref())
            }) {
                operations.push(GitRecordsTransition::Checkout {
                    commit: head.clone(),
                    previous_commit: (!update.old.bytes().all(|byte| byte == b'0'))
                        .then(|| update.old.clone()),
                });
            }
        }
    }
    Ok(operations)
}
