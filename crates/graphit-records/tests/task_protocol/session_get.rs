use super::native::fixture::{Fixture, read_limits, request, write_limits};
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority};
use graphit_core::file_lock::Cancellation;
use graphit_records::{
    task_protocol::{ReadResponse, WriteRequest, WriteResponse, execute_read, execute_write},
    task_store::NativeTaskStore,
};
use serde_json::json;
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_wire_defaults_to_current_and_dispatches_explicit_pinned_history() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
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
    let create:WriteRequest=serde_json::from_value(json!({"operation":"create_session","input":{"idempotency_key":"selected-wire-session","title":"Wire Session","description":"Keep the complete admitted demand","strategy":"Read current and request history"}})).unwrap();
    let write = authority
        .admit_write(&grant, &ctx, create.required_operation())
        .unwrap();
    let WriteResponse::Session(created) =
        execute_write(create, ctx.snapshot(), &write, write_limits())
            .await
            .unwrap()
    else {
        panic!("Session creation")
    };
    let claim: WriteRequest = serde_json::from_value(
        json!({"operation":"claim_session","input":{"id":created.id,"lease_ms":300000}}),
    )
    .unwrap();
    let coordinate = authority
        .admit_write(&grant, &ctx, claim.required_operation())
        .unwrap();
    let WriteResponse::SessionClaim(claim) =
        execute_write(claim, ctx.snapshot(), &coordinate, write_limits())
            .await
            .unwrap()
    else {
        panic!("Session claim")
    };
    let max = std::num::NonZeroUsize::new(1048576).unwrap();
    let current = execute_read(
        serde_json::from_value(json!({"operation":"get_session","input":{"id":created.id}}))
            .unwrap(),
        ctx.snapshot(),
        &read,
        read_limits(),
        max,
    )
    .await
    .unwrap();
    let json = serde_json::to_value(&current).unwrap();
    assert!(json["result"].get("events").is_none());
    assert!(json["result"]["history"].is_null());
    assert_eq!(
        json["result"]["session"],
        serde_json::to_value(&claim.record).unwrap()
    );
    let ReadResponse::GetSession(Some(current)) = current else {
        panic!("Selected Session")
    };
    let history=execute_read(serde_json::from_value(json!({"operation":"get_session","input":{"id":created.id,"history":{"page_size":1,"cursor":current.history_start_cursor}}})).unwrap(),ctx.snapshot(),&read,read_limits(),max).await.unwrap();
    let ReadResponse::GetSession(Some(history)) = history else {
        panic!("Session history")
    };
    let page = history.history.unwrap();
    assert_eq!(page.before, created);
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].record, claim.record);
    assert!(page.next_cursor.is_none());
    assert_eq!(
        NativeTaskStore::read_session(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::SessionGetInput {
                id: created.id,
                history: Some(graphit_records::task_store::SessionDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .history
        .unwrap()
        .events,
        page.events
    );
}
