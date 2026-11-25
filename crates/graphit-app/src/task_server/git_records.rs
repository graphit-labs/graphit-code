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
    let issuer = state.auth.issuer();
    let caller = issuer
        .authenticate_runtime(&credential.bearer, original)
        .map_err(|_| io::Error::from(io::ErrorKind::PermissionDenied))?;
    let live = state.live_projects.snapshot()?;
    let selected = live
        .projects
        .iter()
        .find(|(paths, _, _)| paths.root() == input.project_dir.as_path());
    let Some((paths, id, _)) = selected else {
        if input.event.is_some()
            && !input.initialize
            && !input.restore
            && input.restore_previous.is_none()
        {
            // Git can notify a newly created worktree before it is managed.
            // No project identity, storage, or checkout selection is inferred.
            original.check()?;
            return Ok(json!({"state":"ignored","reason":"unmanaged_checkout"}));
        }
        return Err(io::ErrorKind::PermissionDenied.into());
    };
    let paths = paths.clone();
    let id = *id;
    let project = PublicationScope::project(&paths, id).map_err(io::Error::other)?;
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
        project: Some(ProjectSelection {
            scope: &project,
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
        },
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
    let needs_retained_snapshot = matches!(
        &input.event,
        Some(GitRecordEvent::ReferenceTransaction { .. })
    ) || matches!(
        &input.event,
        Some(GitRecordEvent::Checkout { old, branch: true, .. })
            if old.bytes().all(|byte| byte == b'0')
    );
    if needs_retained_snapshot
        && !graphit_records::git_records::has_git_commit(
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
        // Initial checkout and ref notifications consume retained tags; they
        // cannot reconstruct missing history from present working records.
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
            graphit_core::git_record_event::validate_commit_id(old)?;
            if *branch {
                // Clone/worktree creation has no prior record state to replace.
                // Initialize the selected version without discarding live records.
                operations.push(if old.bytes().all(|byte| byte == b'0') {
                    GitRecordsTransition::Initialize {
                        commit: new.clone(),
                    }
                } else {
                    GitRecordsTransition::Checkout {
                        commit: new.clone(),
                        previous_commit: Some(old.clone()),
                    }
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

#[cfg(test)]
mod transition_tests {
    use super::*;

    #[test]
    fn git_records_checkout_transition_distinguishes_initial_from_destructive_events() {
        use graphit_core::git_record_event::{GitRecordEvent, GitRecordFacts};
        use graphit_records::git_records::GitRecordsTransition;
        let temp = tempfile::tempdir().unwrap();
        let head = "a".repeat(40);
        let facts = GitRecordFacts {
            root: temp.path().to_owned(),
            common_dir: temp.path().to_owned(),
            head: Some(head.clone()),
            parents: vec![],
            replayed_commit: None,
            branch: None,
            default_branch: "main".into(),
        };
        let cancel = Cancellation::default();
        let original = RequestContext::new(
            ContentLimits {
                max_input_bytes: 65536,
                max_output_bytes: 65536,
                max_depth: 32,
                max_nodes: 4096,
                max_scalar_bytes: 4096,
                max_collection_entries: 64,
            },
            &cancel,
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
        let mut input = crate::git_records::Request {
            project_dir: graphit_core::native_path::NativePath::new(temp.path().to_owned())
                .unwrap(),
            event: None,
            restore_previous: None,
            restore: false,
            initialize: false,
        };
        for (old, branch, initial) in [("0".repeat(40), true, true), (head.clone(), true, false)] {
            input.event = Some(GitRecordEvent::Checkout {
                old: old.clone(),
                new: head.clone(),
                branch,
            });
            let mapped = super::transitions(&facts, &input, &original).unwrap();
            if initial {
                assert!(
                    matches!(mapped.as_slice(), [GitRecordsTransition::Initialize { commit }] if commit == &head)
                );
            } else {
                assert!(
                    matches!(mapped.as_slice(), [GitRecordsTransition::Checkout { commit, previous_commit: Some(previous) }] if commit == &head && previous == &old)
                );
            }
        }
        input.event = Some(GitRecordEvent::Checkout {
            old: head.clone(),
            new: head.clone(),
            branch: false,
        });
        assert!(
            super::transitions(&facts, &input, &original)
                .unwrap()
                .is_empty()
        );
        for (old, new) in [
            (String::new(), head.clone()),
            ("0".repeat(40), "b".repeat(40)),
        ] {
            input.event = Some(GitRecordEvent::Checkout {
                old,
                new,
                branch: true,
            });
            assert!(super::transitions(&facts, &input, &original).is_err());
        }
    }
}
