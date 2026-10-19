#[path = "../task_store/fixture.rs"]
pub(super) mod fixture;
use fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::file_lock::Cancellation;
use graphit_records::{gates::TaskStatus, task_protocol::*, task_store::*};
use serde_json::{Value, json};

fn task_input(session: SessionId, key: &str) -> Value {
    json!({"operation":"create_task","input":{"idempotency_key":key,"session_id":session,"parent_id":null,"depends_on":[],"title":key,"description":"Implement the requested behavior","task_type":"feature","priority":2,"schedule":{"start_date":"2024-02-29","end_date":"2024-03-01"},"checks":[{"kind":"acceptance","text":"Required behavior"},{"kind":"test","text":"Affected functional test"}]}})
}
fn target(task: &Task, token: &TransportToken) -> Value {
    json!({"id":task.specification.id,"claim":{"token":token,"expected_revision":task.specification.revision}})
}
fn task(output: WriteResponse) -> Task {
    let WriteResponse::Task(task) = output else {
        panic!("Task result")
    };
    task
}
macro_rules! write {
    ($value:expr,$ctx:ident,$authority:ident,$grant:ident) => {{
        let request: WriteRequest = serde_json::from_value($value).unwrap();
        let admission = $authority
            .admit_write(&$grant, &$ctx, request.required_operation())
            .unwrap();
        execute_write(request, $ctx.snapshot(), &admission, write_limits())
            .await
            .unwrap()
    }};
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_wire_roundtrip_preserves_calendar_checks_dependencies_and_terminal_history() {
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    assert!(matches!(
        execute_read(
            ReadRequest::ReadView,
            ctx.snapshot(),
            &read,
            read_limits(),
            std::num::NonZeroUsize::new(1048576).unwrap()
        )
        .await
        .unwrap(),
        ReadResponse::ReadView(TaskReadView::NoCorpus)
    ));
    let session = write!(
        json!({"operation":"create_session","input":{"idempotency_key":"session","title":"Session","description":"Actual work","strategy":"Implement"}}),
        ctx,
        authority,
        grant
    );
    let WriteResponse::Session(session) = session else {
        panic!("Session result")
    };
    let created = task(write!(
        task_input(session.id, "Task"),
        ctx,
        authority,
        grant
    ));
    let searched = execute_read(
        serde_json::from_value(json!({"operation":"search","input":{
            "text":"requested","filter":{},"page_size":1,"top_k":10,"cursor":null
        }}))
        .unwrap(),
        ctx.snapshot(),
        &read,
        read_limits(),
        std::num::NonZeroUsize::new(1048576).unwrap(),
    )
    .await
    .unwrap();
    let ReadResponse::Search(TaskLexicalSearchRead::Published {
        results,
        total_hits,
        ..
    }) = searched
    else {
        panic!("real writer makes FTS available without a fixture publisher")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(results[0].task.id, created.specification.id);
    let claimed = write!(
        json!({"operation":"claim_task","input":{"id":created.specification.id,"lease_ms":30000}}),
        ctx,
        authority,
        grant
    );
    let serialized_claim = serde_json::to_value(&claimed).unwrap();
    let WriteResponse::TaskClaim(claimed) = claimed else {
        panic!("claim")
    };
    assert_eq!(
        serialized_claim["result"]["token"].as_str().unwrap().len(),
        43
    );
    let token = claimed.token;
    let mut current = claimed.record;
    current = task(write!(
        json!({"operation":"revise_task","input":{"target":target(&current,&token),"lease_ms":30000,"reason":"Adjust due date","priority":null,"schedule":{"due_date":"2024-03-04"},"add_checks":[]}}),
        ctx,
        authority,
        grant
    ));
    let due = current.specification.schedule;
    for check in current.specification.checks.clone() {
        current = task(write!(
            json!({"operation":"verify_task_check","input":{"target":target(&current,&token),"lease_ms":30000,"check_id":check.id,"passed":true,"evidence":"Actual qualified result"}}),
            ctx,
            authority,
            grant
        ));
    }
    current = task(write!(
        json!({"operation":"revise_task","input":{"target":target(&current,&token),"lease_ms":30000,"reason":"Change priority","priority":1,"add_checks":[]}}),
        ctx,
        authority,
        grant
    ));
    assert_eq!(current.specification.schedule, due);
    assert!(
        current
            .specification
            .checks
            .iter()
            .all(|check| check.verification.is_some())
    );
    current = task(write!(
        json!({"operation":"revise_task","input":{"target":target(&current,&token),"lease_ms":30000,"reason":"Clear schedule","priority":null,"schedule":null,"add_checks":[]}}),
        ctx,
        authority,
        grant
    ));
    assert!(current.specification.schedule.is_none());
    let check = current.specification.checks[0].id;
    current = task(write!(
        json!({"operation":"supersede_task_check","input":{"target":target(&current,&token),"lease_ms":30000,"check_id":check,"reason":"Replace criterion","replacement":{"text":"Replacement criterion","kind":null}}}),
        ctx,
        authority,
        grant
    ));
    let replacement = current.specification.checks.last().unwrap().id;
    current = task(write!(
        json!({"operation":"verify_task_check","input":{"target":target(&current,&token),"lease_ms":30000,"check_id":replacement,"passed":true,"evidence":"Replacement verified"}}),
        ctx,
        authority,
        grant
    ));
    current = task(write!(
        json!({"operation":"set_task_flag","input":{"target":target(&current,&token),"lease_ms":30000,"flagged":true,"reason":"Review result"}}),
        ctx,
        authority,
        grant
    ));
    current = task(write!(
        json!({"operation":"set_task_flag","input":{"target":target(&current,&token),"lease_ms":30000,"flagged":false,"reason":"Result reviewed"}}),
        ctx,
        authority,
        grant
    ));
    let mut stale = target(&current, &token);
    stale["claim"]["expected_revision"] = json!(current.specification.revision - 1);
    let stale: WriteRequest =
        serde_json::from_value(json!({"operation":"complete_task","input":stale})).unwrap();
    let owned = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    assert!(matches!(
        execute_write(stale, ctx.snapshot(), &owned, write_limits()).await,
        Err(TaskStoreError::RevisionConflict)
    ));
    let mut wrong = target(&current, &token);
    wrong["claim"]["token"] =
        serde_json::to_value(TransportToken::from_claim(ClaimToken::from_bytes([0; 32]))).unwrap();
    let wrong: WriteRequest =
        serde_json::from_value(json!({"operation":"complete_task","input":wrong})).unwrap();
    assert!(matches!(
        execute_write(wrong, ctx.snapshot(), &owned, write_limits()).await,
        Err(TaskStoreError::FenceMismatch)
    ));
    current = task(write!(
        json!({"operation":"complete_task","input":target(&current,&token)}),
        ctx,
        authority,
        grant
    ));
    assert_eq!(current.status, TaskStatus::Completed);
    let mut dependent = task_input(session.id, "Dependent");
    dependent["input"]["depends_on"] = json!([current.specification.id]);
    let dependent = task(write!(dependent, ctx, authority, grant));
    let page = execute_read(
        serde_json::from_value(
            json!({"operation":"catalogue","input":{"filter":{},"page_size":20,"cursor":null}}),
        )
        .unwrap(),
        ctx.snapshot(),
        &read,
        read_limits(),
        std::num::NonZeroUsize::new(1048576).unwrap(),
    )
    .await
    .unwrap();
    let ReadResponse::Catalogue(page) = page else {
        panic!("catalogue")
    };
    assert_eq!(page.results.len(), 2);
    let summary = page
        .results
        .iter()
        .find(|t| t.id == dependent.specification.id)
        .unwrap();
    assert_eq!(summary.depends_on, vec![current.specification.id]);
    assert!(summary.schedule.is_some());
    let detail = execute_read(
        ReadRequest::GetTask(TaskGetInput {
            id: current.specification.id,
            history: Some(TaskDetailPageInput {
                page_size: std::num::NonZeroUsize::new(100).unwrap(),
                cursor: None,
            }),
            comments: None,
        }),
        ctx.snapshot(),
        &read,
        read_limits(),
        std::num::NonZeroUsize::new(1048576).unwrap(),
    )
    .await
    .unwrap();
    let serialized = serde_json::to_value(&detail).unwrap();
    assert!(
        !serde_json::to_string(&serialized)
            .unwrap()
            .contains("token")
    );
    let ReadResponse::GetTask(Some(detail)) = detail else {
        panic!("detail")
    };
    assert_eq!(detail.task, current);
    assert!(detail.history.as_ref().unwrap().events.len() > 5);
    assert!(detail.creation.record.specification.schedule.is_some());
    let cancelled = task(write!(
        json!({"operation":"cancel_task","input":{"id":dependent.specification.id,"claim":null,"reason":"Withdraw remaining work"}}),
        ctx,
        authority,
        grant
    ));
    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    let session_claim = write!(
        json!({"operation":"claim_session","input":{"id":session.id,"lease_ms":30000}}),
        ctx,
        authority,
        grant
    );
    let WriteResponse::SessionClaim(session_claim) = session_claim else {
        panic!("Session claim")
    };
    let done = write!(
        json!({"operation":"complete_session","input":{"id":session.id,"claim":{"token":session_claim.token,"expected_revision":session_claim.record.revision}}}),
        ctx,
        authority,
        grant
    );
    let WriteResponse::Session(done) = done else {
        panic!("Session completion")
    };
    assert_eq!(done.status, TaskStatus::Completed);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_authenticated_admission_and_original_cancellation_remain_required() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (_, reference, _run) = f.issue(&issuer, &ctx);
    let (bearer, _run2) = f.issue_registered(&issuer, &ctx, &reference);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
        .unwrap();
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    let req:WriteRequest=serde_json::from_value(json!({"operation":"create_session","input":{"idempotency_key":"runtime","title":"Session","description":"Original request","strategy":"Actual producer"}})).unwrap();
    let result = execute_write(req, ctx.snapshot(), &write, write_limits())
        .await
        .unwrap();
    let WriteResponse::Session(session) = result else {
        panic!("Session")
    };
    let different = Cancellation::default();
    let other = request(&different);
    assert!(matches!(
        execute_read(
            ReadRequest::ReadView,
            other.snapshot(),
            &read,
            read_limits(),
            std::num::NonZeroUsize::new(1048576).unwrap()
        )
        .await,
        Err(TaskStoreError::DifferentRequest)
    ));
    let role_read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let owned = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    let req: WriteRequest =
        serde_json::from_value(task_input(session.id, "Forbidden create")).unwrap();
    assert!(matches!(
        execute_write(req, ctx.snapshot(), &owned, write_limits()).await,
        Err(TaskStoreError::WrongAdmission)
    ));
    let created: WriteRequest =
        serde_json::from_value(task_input(session.id, "Expiring owner")).unwrap();
    let created = task(
        execute_write(created, ctx.snapshot(), &write, write_limits())
            .await
            .unwrap(),
    );
    let claimed = write!(
        json!({"operation":"claim_task","input":{"id":created.specification.id,"lease_ms":1000}}),
        ctx,
        authority,
        grant
    );
    let WriteResponse::TaskClaim(claimed) = claimed else {
        panic!("claim")
    };
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let expired: WriteRequest = serde_json::from_value(json!({"operation":"heartbeat_task","input":{"target":target(&claimed.record,&claimed.token),"lease_ms":30000}})).unwrap();
    assert!(matches!(
        execute_write(expired, ctx.snapshot(), &owned, write_limits()).await,
        Err(TaskStoreError::LeaseExpired)
    ));
    cancel.cancel();
    assert!(
        execute_read(
            ReadRequest::ReadView,
            ctx.snapshot(),
            &role_read,
            read_limits(),
            std::num::NonZeroUsize::new(1048576).unwrap()
        )
        .await
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn references_actual_wire_preserves_omission_clear_literal_tuples_and_rejects_invalid_tail_before_birth()
 {
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let invalids = [
        Value::Null,
        json!({"wrong":"object"}),
        json!([{"target":{"type":"memory","id":"first"},"relation":"supports"},{"target":{"type":"memory","id":"last","context":"invalid"},"relation":"supports"}]),
        json!([{"target":{"type":"memory","id":"id"},"relation":"supports","field":null}]),
    ];
    for refs in invalids {
        let request:WriteRequest=serde_json::from_value(json!({"operation":"create_session","input":{"idempotency_key":"invalid","title":"S","description":"D","strategy":"Implement","references":refs}})).unwrap();
        let admission = authority
            .admit_write(&grant, &ctx, request.required_operation())
            .unwrap();
        assert!(
            execute_write(request, ctx.snapshot(), &admission, write_limits())
                .await
                .is_err()
        );
        assert!(!f.project.runtime_root().join("task").exists());
    }
    let WriteResponse::Session(session) = write!(
        json!({"operation":"create_session","input":{"idempotency_key":"s","title":"S","description":"D","strategy":"Implement","references":[]}}),
        ctx,
        authority,
        grant
    ) else {
        panic!("session")
    };
    let mut input = task_input(session.id, "literal");
    input["input"]["references"] = json!([{"target":{"type":"memory","id":" literal\nid "},"relation":" rel\n ","field":" field "}]);
    let created = task(write!(input, ctx, authority, grant));
    assert_eq!(created.references[0].id, " literal\nid ");
    let WriteResponse::TaskClaim(claimed) = write!(
        json!({"operation":"claim_task","input":{"id":created.specification.id,"lease_ms":120000}}),
        ctx,
        authority,
        grant
    ) else {
        panic!("claim")
    };
    let kept = task(write!(
        json!({"operation":"progress_task","input":{"target":target(&claimed.record,&claimed.token),"lease_ms":120000,"summary":"Keep","next_step":"Clear"}}),
        ctx,
        authority,
        grant
    ));
    assert_eq!(kept.references, created.references);
    let cleared = task(write!(
        json!({"operation":"revise_task","input":{"target":target(&kept,&claimed.token),"lease_ms":120000,"reason":"Clear only","references":[]}}),
        ctx,
        authority,
        grant
    ));
    assert!(cleared.references.is_empty());
    assert_eq!(cleared.specification.checks, kept.specification.checks);
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput::current(cleared.specification.id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        detail.last_event.as_ref().unwrap().references_update,
        TaskReferenceUpdate::Replace(vec![])
    );
    assert!(serde_json::from_value::<WriteRequest>(json!({"operation":"heartbeat_task","input":{"target":target(&cleared,&claimed.token),"lease_ms":120000,"references":[]}})).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commit_metadata_cas_fences_amend_and_terminal_history() {
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let WriteResponse::Session(session) = write!(
        json!({"operation":"create_session","input":{
        "idempotency_key":"commit-session","title":"Commit metadata","description":"Track delivered work","strategy":"Preserve evidence"}}),
        ctx,
        authority,
        grant
    ) else {
        panic!("session")
    };
    let mut creation = task_input(session.id, "commit-task");
    creation["input"]["commit"] = json!("a".repeat(40));
    let created = task(write!(creation.clone(), ctx, authority, grant));
    assert_eq!(created.commit.as_deref(), Some("a".repeat(40).as_str()));
    let WriteResponse::TaskClaim(claim) = write!(
        json!({"operation":"claim_task","input":{"id":created.specification.id,"lease_ms":30000}}),
        ctx,
        authority,
        grant
    ) else {
        panic!("claim")
    };
    let mut current = claim.record;
    let token = claim.token;
    let update = |current: &Task, commit: Value, fence: Option<Value>| {
        json!({"operation":"set_task_commit","input":{
        "id": current.specification.id, "expected_revision": current.specification.revision,
        "claim":fence,"commit":commit,"reason":"Record successful commit"}})
    };
    let request: WriteRequest =
        serde_json::from_value(update(&current, json!("b".repeat(40)), None)).unwrap();
    let admission = authority
        .admit_write(&grant, &ctx, request.required_operation())
        .unwrap();
    assert!(matches!(
        execute_write(request, ctx.snapshot(), &admission, write_limits()).await,
        Err(TaskStoreError::FenceMismatch)
    ));
    let mut wrong_fence = target(&current, &token)["claim"].clone();
    wrong_fence["token"] = json!("A".repeat(43));
    let invalid: WriteRequest =
        serde_json::from_value(update(&current, json!("b".repeat(40)), Some(wrong_fence))).unwrap();
    let owned = authority
        .admit_write(&grant, &ctx, invalid.required_operation())
        .unwrap();
    assert!(matches!(
        execute_write(invalid, ctx.snapshot(), &owned, write_limits()).await,
        Err(TaskStoreError::FenceMismatch)
    ));
    let mut mismatched_revision = target(&current, &token)["claim"].clone();
    mismatched_revision["expected_revision"] = json!(current.specification.revision + 1);
    let invalid: WriteRequest = serde_json::from_value(update(
        &current,
        json!("b".repeat(40)),
        Some(mismatched_revision),
    ))
    .unwrap();
    assert!(matches!(
        execute_write(invalid, ctx.snapshot(), &owned, write_limits()).await,
        Err(TaskStoreError::RevisionConflict)
    ));
    let fence = target(&current, &token)["claim"].clone();
    let previous = current.clone();
    current = task(write!(
        update(&current, json!("b".repeat(40)), Some(fence)),
        ctx,
        authority,
        grant
    ));
    assert_eq!(current.status, previous.status);
    assert_eq!(current.ownership, previous.ownership);
    assert_eq!(current.specification.checks, previous.specification.checks);
    for check in current.specification.checks.clone() {
        current = task(write!(
            json!({"operation":"verify_task_check","input":{"target":target(&current,&token),"lease_ms":30000,"check_id":check.id,"passed":true,"evidence":"Qualified behavior"}}),
            ctx,
            authority,
            grant
        ));
    }
    current = task(write!(
        json!({"operation":"complete_task","input":target(&current,&token)}),
        ctx,
        authority,
        grant
    ));
    let completed = current.clone();
    let sha = "c".repeat(64);
    current = task(write!(
        update(&current, json!(sha), None),
        ctx,
        authority,
        grant
    ));
    assert_eq!(current.status, TaskStatus::Completed);
    assert_eq!(current.ownership, completed.ownership);
    assert_eq!(current.specification.checks, completed.specification.checks);
    assert_eq!(current.progress, completed.progress);
    assert_eq!(current.commit.as_deref(), Some(sha.as_str()));
    // The creation retry compares the immutable creation, not the amended commit.
    let retry = task(write!(creation, ctx, authority, grant));
    assert_eq!(retry, current);
    let noop = task(write!(
        update(&current, json!(sha), None),
        ctx,
        authority,
        grant
    ));
    assert_eq!(noop, current);
    let stale: WriteRequest =
        serde_json::from_value(update(&completed, json!("d".repeat(40)), None)).unwrap();
    let admission = authority
        .admit_write(&grant, &ctx, stale.required_operation())
        .unwrap();
    assert!(matches!(
        execute_write(stale, ctx.snapshot(), &admission, write_limits()).await,
        Err(TaskStoreError::RevisionConflict)
    ));
    for invalid in ["", "abc123", "HEAD", &"A".repeat(40), &"f".repeat(41)] {
        let request: WriteRequest =
            serde_json::from_value(update(&current, json!(invalid), None)).unwrap();
        assert!(matches!(
            execute_write(request, ctx.snapshot(), &admission, write_limits()).await,
            Err(TaskStoreError::Invalid(_))
        ));
    }
    let mut missing = update(&current, Value::Null, None);
    missing["input"].as_object_mut().unwrap().remove("commit");
    let missing: WriteRequest = serde_json::from_value(missing).unwrap();
    assert!(matches!(
        execute_write(missing, ctx.snapshot(), &admission, write_limits()).await,
        Err(TaskStoreError::Invalid(_))
    ));
    let before_clear = current.clone();
    current = task(write!(
        update(&current, Value::Null, None),
        ctx,
        authority,
        grant
    ));
    assert_eq!(current.commit, None);
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        serde_json::from_value(json!({"id":current.specification.id,"history":{"page_size":100}}))
            .unwrap(),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(detail.task, current);
    assert_eq!(detail.creation.record.commit, created.commit);
    assert!(
        detail
            .history
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(
                |event| matches!(event.kind, LifecycleKind::CommitUpdated { .. })
                    && event.record.commit == before_clear.commit
            )
    );
    assert_eq!(
        detail
            .history
            .as_ref()
            .unwrap()
            .events
            .last()
            .unwrap()
            .record
            .status,
        TaskStatus::Completed
    );
    cancellation.cancel();
    let request: WriteRequest =
        serde_json::from_value(update(&current, json!("d".repeat(40)), None)).unwrap();
    assert!(
        execute_write(request, ctx.snapshot(), &admission, write_limits())
            .await
            .is_err()
    );
}
