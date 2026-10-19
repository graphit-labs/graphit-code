use super::native::fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::file_lock::Cancellation;
use graphit_records::{task_protocol::*, task_store::*};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comments_actual_dispatcher_literal_body_and_closed_wire() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let owned = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let session = execute_write(serde_json::from_value(json!({"operation":"create_session","input":{"idempotency_key":"wire-session","title":"Session","description":"Actual comments","strategy":"Implement"}})).unwrap(), ctx.snapshot(), &write, write_limits()).await.unwrap();
    let WriteResponse::Session(session) = session else {
        panic!("session")
    };
    let task = execute_write(serde_json::from_value(json!({"operation":"create_task","input":{"idempotency_key":"wire-task","session_id":session.id,"parent_id":null,"depends_on":[],"title":"Task","description":"Actual comments","task_type":"feature","priority":2,"schedule":null,"checks":[{"kind":"acceptance","text":"Required behavior"},{"kind":"test","text":"Affected test"}]}})).unwrap(),ctx.snapshot(),&write,write_limits()).await.unwrap();
    let WriteResponse::Task(task) = task else {
        panic!("task")
    };
    let id = task.specification.id;
    let claim = execute_write(
        serde_json::from_value(
            json!({"operation":"claim_task","input":{"id":id,"lease_ms":60000}}),
        )
        .unwrap(),
        ctx.snapshot(),
        &owned,
        write_limits(),
    )
    .await
    .unwrap();
    let WriteResponse::TaskClaim(claim) = claim else {
        panic!("claim")
    };
    let body = "  **Decision**\n\n    literal code\n";
    let value = json!({"operation":"add_task_comment","input":{"target":{"id":id,"claim":{"token":claim.token,"expected_revision":claim.record.specification.revision}},"lease_ms":30000,"idempotency_key":"comment","kind":"decision","body":body}});
    let decoded: WriteRequest = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded.required_operation(), ProjectWriteOperation::OwnTask);
    let result = execute_write(decoded, ctx.snapshot(), &owned, write_limits())
        .await
        .unwrap();
    let wire = serde_json::to_value(&result).unwrap();
    assert_eq!(wire["kind"], "comment");
    assert_eq!(wire["result"]["kind"], "created");
    assert_eq!(wire["result"]["result"]["comment"]["body"], body);
    assert!(!wire.to_string().contains("token"));
    let WriteResponse::Comment(first) = result else {
        panic!("comment")
    };
    let TaskCommentOutcome::Created { comment, .. } = *first else {
        panic!("created")
    };
    let repeated = execute_write(
        serde_json::from_value(value.clone()).unwrap(),
        ctx.snapshot(),
        &owned,
        write_limits(),
    )
    .await
    .unwrap();
    let WriteResponse::Comment(repeated) = repeated else {
        panic!("comment")
    };
    let TaskCommentOutcome::Existing { comment: again, .. } = *repeated else {
        panic!("existing")
    };
    assert_eq!(comment, again);
    let result = execute_read(
        serde_json::from_value(
            json!({"operation":"get_task","input":{"id":id,"comments":{"page_size":10}}}),
        )
        .unwrap(),
        ctx.snapshot(),
        &read,
        read_limits(),
        std::num::NonZeroUsize::new(1048576).unwrap(),
    )
    .await
    .unwrap();
    let ReadResponse::GetTask(Some(detail)) = result else {
        panic!("detail")
    };
    assert_eq!(detail.comments.as_ref().unwrap().comments, vec![comment]);
    assert!(!serde_json::to_string(&detail).unwrap().contains("token"));
    for field in ["actor", "scope", "branch", "at", "sequence"] {
        let mut invalid = value.clone();
        invalid["input"][field] = json!("untrusted");
        assert!(serde_json::from_value::<WriteRequest>(invalid).is_err());
    }
    for (field, bad) in [("kind", json!("unknown")), ("lease_ms", json!(0))] {
        let mut invalid = value.clone();
        invalid["input"][field] = bad;
        assert!(serde_json::from_value::<WriteRequest>(invalid).is_err());
    }
}
