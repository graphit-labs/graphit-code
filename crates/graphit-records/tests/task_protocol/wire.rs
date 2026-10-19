use graphit_auth::ProjectWriteOperation;
use graphit_records::{task_protocol::*, task_spec::FieldUpdate, task_store::ClaimToken};
use serde_json::{Value, json};
const ID: &str = "01KSH1CRFFG8Z74B5ZS78WW808";
#[test]
fn task_get_selects_only_explicit_closed_positive_detail_pages() {
    let current: ReadRequest =
        serde_json::from_value(json!({"operation":"get_task","input":{"id":ID}})).unwrap();
    let ReadRequest::GetTask(current) = current else {
        panic!("Get")
    };
    assert!(current.history.is_none() && current.comments.is_none());
    for section in ["history", "comments"] {
        let mut input = json!({"operation":"get_task","input":{"id":ID}});
        input["input"][section] = json!({"page_size":1});
        assert!(serde_json::from_value::<ReadRequest>(input.clone()).is_ok());
        input["input"][section] = json!({"page_size":0});
        assert!(serde_json::from_value::<ReadRequest>(input.clone()).is_err());
        input["input"][section] = json!({"page_size":1,"branch":"caller-controlled"});
        assert!(serde_json::from_value::<ReadRequest>(input.clone()).is_err());
        input["input"][section] = json!({"cursor":"invalid"});
        assert!(serde_json::from_value::<ReadRequest>(input).is_err());
    }
}
fn revision() -> Value {
    json!({"operation":"revise_task","input":{
        "target":{"id":ID,"claim":{"token":serde_json::to_value(TransportToken::from_claim(ClaimToken::from_bytes([1;32]))).unwrap(),"expected_revision":2}},
        "lease_ms":30000,"reason":"Review changed requirements","priority":null,"add_checks":[]
    }})
}
#[test]
fn field_presence_preserves_native_calendar_semantics() {
    let omitted = revision();
    let req: WriteRequest = serde_json::from_value(omitted.clone()).unwrap();
    let WriteRequest::ReviseTask(input) = &req else {
        panic!("revision")
    };
    assert!(matches!(input.schedule, FieldUpdate::Keep));
    assert_eq!(serde_json::to_value(req).unwrap(), omitted);
    let mut clear = omitted.clone();
    clear["input"]["schedule"] = Value::Null;
    let req: WriteRequest = serde_json::from_value(clear.clone()).unwrap();
    let WriteRequest::ReviseTask(input) = &req else {
        panic!("revision")
    };
    assert!(matches!(input.schedule, FieldUpdate::Set(None)));
    assert_eq!(serde_json::to_value(req).unwrap(), clear);
    let mut dated = omitted;
    dated["input"]["schedule"] = json!({"start_date":"2024-02-29","end_date":"2024-03-01"});
    let req: WriteRequest = serde_json::from_value(dated.clone()).unwrap();
    assert_eq!(serde_json::to_value(req).unwrap(), dated);
}
#[test]
fn reject_unknown_duplicate_invalid_calendar_and_identifier_inputs() {
    for schedule in [
        json!({}),
        json!({"due_date":"2023-02-29"}),
        json!({"start_date":"2024-03-02","end_date":"2024-03-01"}),
        json!({"due_date":"2024-02-29","extra":true}),
    ] {
        let mut req = revision();
        req["input"]["schedule"] = schedule;
        assert!(serde_json::from_value::<WriteRequest>(req).is_err());
    }
    let mut req = revision();
    req["input"]["actor"] = json!("caller supplied actor");
    assert!(serde_json::from_value::<WriteRequest>(req).is_err());
    let mut req = revision();
    req["input"]["target"]["id"] = json!("tsk-old");
    assert!(serde_json::from_value::<WriteRequest>(req).is_err());
    let mut req = revision();
    req["input"]["lease_ms"] = json!(0);
    assert!(serde_json::from_value::<WriteRequest>(req).is_err());
    let dup = serde_json::to_string(&revision())
        .unwrap()
        .replace("\"reason\":", "\"reason\":\"first\",\"reason\":");
    assert!(serde_json::from_str::<WriteRequest>(&dup).is_err());
}
#[test]
fn participant_nonce_is_canonical_base64url_and_not_an_admission_selector() {
    let original = TransportToken::from_claim(ClaimToken::from_bytes([0xff; 32]));
    let value = serde_json::to_value(&original).unwrap();
    let decoded: TransportToken = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded.claim().as_bytes(), original.claim().as_bytes());
    let text = value.as_str().unwrap();
    assert_eq!(text.len(), 43);
    assert!(!text.contains('='));
    for invalid in [
        "".to_owned(),
        text[..42].to_owned(),
        format!("{text}="),
        "A".repeat(44),
        "!".repeat(43),
    ] {
        assert!(serde_json::from_value::<TransportToken>(json!(invalid)).is_err());
    }
    let mut wrong = revision();
    wrong["input"]["scope"] = json!({"branch":"main"});
    assert!(serde_json::from_value::<WriteRequest>(wrong).is_err());
}
#[test]
fn classification_is_derived_from_the_actual_operation() {
    let create:WriteRequest=serde_json::from_value(json!({"operation":"create_session","input":{"idempotency_key":"a","title":"A","description":"B","strategy":"C"}})).unwrap();
    assert_eq!(create.required_operation(), ProjectWriteOperation::Task);
    let owned: WriteRequest = serde_json::from_value(revision()).unwrap();
    assert_eq!(owned.required_operation(), ProjectWriteOperation::OwnTask);
    let session: WriteRequest = serde_json::from_value(
        json!({"operation":"claim_session","input":{"id":ID,"lease_ms":1000}}),
    )
    .unwrap();
    assert_eq!(
        session.required_operation(),
        ProjectWriteOperation::CoordinateTask
    );
    let cancel: WriteRequest = serde_json::from_value(
        json!({"operation":"cancel_task","input":{"id":ID,"claim":null,"reason":"Withdraw work"}}),
    )
    .unwrap();
    assert_eq!(cancel.required_operation(), ProjectWriteOperation::Task);
}

#[test]
fn session_revision_preserves_partial_fields_and_coordinator_classification() {
    let mut request = revision();
    request["operation"] = json!("revise_session");
    let fields = request["input"].as_object_mut().unwrap();
    fields.remove("priority");
    fields.remove("add_checks");
    let decoded: WriteRequest = serde_json::from_value(request.clone()).unwrap();
    assert_eq!(
        decoded.required_operation(),
        ProjectWriteOperation::CoordinateTask
    );
    let WriteRequest::ReviseSession(input) = &decoded else {
        panic!("Session revision operation");
    };
    assert!(matches!(input.title, FieldUpdate::Keep));
    assert!(matches!(input.description, FieldUpdate::Keep));
    assert!(matches!(input.strategy, FieldUpdate::Keep));
    assert_eq!(serde_json::to_value(decoded).unwrap(), request);
    for field in ["title", "description", "strategy"] {
        let mut changed = request.clone();
        changed["input"][field] = json!("Reconciled demand");
        changed["input"]["references"] = json!([]);
        let decoded: WriteRequest = serde_json::from_value(changed.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), changed);
        changed["input"][field] = Value::Null;
        assert!(serde_json::from_value::<WriteRequest>(changed).is_err());
    }
    for field in ["actor", "id", "scope", "created_at", "status", "task_type"] {
        let mut invalid = request.clone();
        invalid["input"][field] = json!("client cannot choose this");
        assert!(serde_json::from_value::<WriteRequest>(invalid).is_err());
    }
    let duplicate = serde_json::to_string(&request)
        .unwrap()
        .replace("\"reason\":", "\"reason\":\"first\",\"reason\":");
    assert!(serde_json::from_str::<WriteRequest>(&duplicate).is_err());
}

#[test]
fn session_get_has_only_explicit_closed_positive_history_pages() {
    for history in [None, Some(Value::Null)] {
        let mut value = json!({"operation":"get_session","input":{"id":ID}});
        if let Some(history) = history {
            value["input"]["history"] = history;
        }
        let ReadRequest::GetSession(input) = serde_json::from_value::<ReadRequest>(value).unwrap()
        else {
            panic!("Session Get")
        };
        assert!(input.history.is_none());
    }
    for history in [json!({"page_size":1}), json!({"page_size":2,"cursor":null})] {
        assert!(
            serde_json::from_value::<ReadRequest>(
                json!({"operation":"get_session","input":{"id":ID,"history":history}})
            )
            .is_ok()
        );
    }
    for history in [
        json!({"page_size":0}),
        json!({"page_size":1,"branch":"untrusted"}),
        json!({"page_size":1,"cursor":"invalid"}),
        json!({"cursor":null}),
    ] {
        assert!(
            serde_json::from_value::<ReadRequest>(
                json!({"operation":"get_session","input":{"id":ID,"history":history}})
            )
            .is_err()
        );
    }
    assert!(
        serde_json::from_value::<ReadRequest>(
            json!({"operation":"get_session","input":{"id":ID,"comments":{"page_size":1}}})
        )
        .is_err()
    );
}
