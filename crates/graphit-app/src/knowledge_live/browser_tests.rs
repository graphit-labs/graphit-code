//! Real signed OAuth/USER admissions over the actual HTTP router; no native child claim.
use super::*;
use crate::{
    knowledge_live::{CreateRequest, HistoryLimits, LiveSessionService},
    task_server::{
        browser::{
            live::{Input, Output},
            user_memory_fixture,
        },
        tests::fixture_with_user_permissions,
    },
};
use graphit_auth::Permission;
use graphit_core::{module_settings::Module, native_document_file::NativeDocumentFileLimits};
use graphit_hub::{NativeBinding, policy::RequestContext};

fn live_request() -> CreateRequest {
    serde_json::from_value(json!({
        "request_id":"web-history", "project":{"project_id":"selected-project","project_version":"main"},
        "binding":NativeBinding::KiroV3,
        "agent":{"source":"hub","project_id":null,"instance_id":null,"project_kind":null,"type":"agent","id":"documentary-agent","version":"1.0.0"},
        "artifacts":[], "title":"Actual USER history", "prompt":null, "model":"fixture-model"
    })).unwrap()
}
async fn live_fixture(permissions: &[Permission]) -> (tempfile::TempDir, Fixture) {
    let (temp, server, raw) =
        fixture_with_user_permissions(&[Permission::ManageAuthProfiles], &[], Some(permissions));
    let limits = HistoryLimits::capture(
        NativeDocumentFileLimits::new(2 * 1024 * 1024, 4096, 128, 8192).unwrap(),
        server.browser_policy().0,
    )
    .unwrap();
    let server = server
        .with_live_sessions(Arc::new(LiveSessionService::new(limits)))
        .unwrap();
    let fixture = Fixture::with_tasks(policy(), Some((server, raw))).await;
    let (server, raw) = fixture.tasks.as_ref().unwrap();
    user_memory_fixture::provider(server, raw, &fixture.issuer.origin);
    (temp, fixture)
}
async fn seed_live(f: &Fixture) {
    let (server, _) = f.tasks.as_ref().unwrap();
    let entry = f
        .service
        .0
        .sessions
        .lock()
        .await
        .values()
        .next()
        .cloned()
        .unwrap();
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        server.browser_policy().0,
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let shared = &f.service.0;
    let credentials = shared
        .credentials(&entry, original.snapshot())
        .await
        .unwrap_or_else(|_| panic!("real session credentials unavailable"));
    let token = zeroize::Zeroizing::new(credentials.access_token().to_owned());
    drop(credentials);
    let flow = shared
        .flow(entry.provider)
        .unwrap_or_else(|_| panic!("real OAuth flow unavailable"));
    let (provider, verifier) = &shared.providers[entry.provider];
    let caller = flow
        .verify_access(
            &entry.registered,
            &token,
            BrokerRequestContext::new(original.snapshot()),
        )
        .await
        .unwrap();
    let output = server
        .live_broker(
            (&caller, verifier, provider),
            &original,
            Input::Seed(Box::new(live_request())),
            &|| true,
        )
        .await;
    match output {
        Ok(Output::History(bytes)) => assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap()["id"],
            "web-history"
        ),
        Ok(Output::Closed(_)) => panic!("unexpected subscriber result"),
        Err(error) => {
            let response = error.into_response();
            panic!("seed admission status {}", response.status());
        }
    }
}
async fn call(f: &Fixture, cookie: &str, method: reqwest::Method, path: &str) -> reqwest::Response {
    f.client
        .request(method, format!("{}{path}", f.origin))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap()
}
#[tokio::test]
async fn live_http_user_history_sse_reconnect_close_remove_and_session_denial() {
    let (_temp, f) = live_fixture(&[
        Permission::ReadUser,
        Permission::MutateUser,
        Permission::Read(Module::Knowledge),
        Permission::MutateModule(Module::Knowledge),
    ])
    .await;
    let cookie = f.login().await;
    let same_origin = f
        .client
        .get(format!("{}/api/live/sessions", f.origin))
        .header(header::COOKIE, &cookie)
        .header("sec-fetch-site", "same-origin")
        .header("sec-fetch-mode", "cors")
        .send()
        .await
        .unwrap();
    let status = same_origin.status();
    let body = same_origin.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "sanitized Live response: {body}");
    for site in [None, Some("cross-site")] {
        let mut request = f
            .client
            .get(format!("{}/api/live/sessions", f.origin))
            .header(header::COOKIE, &cookie);
        if let Some(site) = site {
            request = request
                .header("sec-fetch-site", site)
                .header("sec-fetch-mode", "cors");
        }
        assert_eq!(
            request.send().await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
    let bad_origin = f
        .client
        .get(format!("{}/api/live/sessions", f.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, "https://different.example")
        .header("sec-fetch-site", "same-origin")
        .header("sec-fetch-mode", "cors")
        .send()
        .await
        .unwrap();
    assert_eq!(bad_origin.status(), StatusCode::FORBIDDEN);
    let mutation = f
        .client
        .post(format!("{}/api/live/sessions/absent/cancel", f.origin))
        .header(header::COOKIE, &cookie)
        .header("sec-fetch-site", "same-origin")
        .header("sec-fetch-mode", "cors")
        .send()
        .await
        .unwrap();
    assert_eq!(mutation.status(), StatusCode::FORBIDDEN);
    seed_live(&f).await;
    // Owner history does not consult PROJECT or UserInfo. Native execution still needs PROJECT separately.
    f.issuer.project_grant.store(false, Ordering::Release);
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let calls = f.issuer.project_calls.load(Ordering::SeqCst);
    let response = call(&f, &cookie, reqwest::Method::GET, "/api/live/sessions").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["sessions"][0]["id"],
        "web-history"
    );
    let response = call(
        &f,
        &cookie,
        reqwest::Method::GET,
        "/api/live/sessions/web-history",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value = response.json().await.unwrap();
    let generation = value["session"]["cursor"]["generation"].as_str().unwrap();
    assert_eq!(value["events"][0]["sequence"], "1");
    let wrong = call(
        &f,
        &cookie,
        reqwest::Method::GET,
        "/api/live/sessions/web-history/events?generation=abcdef&sequence=1",
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::BAD_REQUEST);
    let path = format!("/api/live/sessions/web-history/events?generation={generation}&sequence=1");
    let subscriber = call(&f, &cookie, reqwest::Method::GET, &path).await;
    assert_eq!(subscriber.status(), StatusCode::OK);
    let closed = call(
        &f,
        &cookie,
        reqwest::Method::POST,
        "/api/live/sessions/web-history/close",
    )
    .await;
    assert_eq!(closed.status(), StatusCode::OK);
    assert_eq!(
        closed.json::<Value>().await.unwrap()["session"]["state"],
        "closed"
    );
    let body = tokio::time::timeout(Duration::from_secs(5), subscriber.text())
        .await
        .unwrap()
        .unwrap();
    assert!(body.contains("event: opened"));
    assert!(body.contains(&format!("id: {generation}:2")));
    assert!(!body.contains(&format!("id: {generation}:1")));
    assert!(body.contains("event: closed"));
    let path = format!(
        "/api/live/sessions/web-history/events?generation={generation}&sequence={}",
        u64::MAX
    );
    let response = call(&f, &cookie, reqwest::Method::GET, &path).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    assert!(body.contains("event: closed"));
    assert!(!body.contains("event: event"));
    assert_eq!(
        call(
            &f,
            &cookie,
            reqwest::Method::DELETE,
            "/api/live/sessions/web-history"
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        call(
            &f,
            &cookie,
            reqwest::Method::GET,
            "/api/live/sessions/web-history"
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), calls);
    assert_eq!(
        call(&f, "", reqwest::Method::GET, "/api/live/sessions")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    // Use the exact production body stream, left unpolled until a real OAuth logout.
    let entry = f
        .service
        .0
        .sessions
        .lock()
        .await
        .values()
        .next()
        .cloned()
        .unwrap();
    let cancellation = Cancellation::default();
    let (_send, progress) = tokio::sync::mpsc::channel(1);
    let (_done, terminal) = oneshot::channel();
    let queued = crate::browser_auth::live::Subscriber {
        first: Some(
            axum::response::sse::Event::default()
                .event("opened")
                .data("queued session payload"),
        ),
        progress,
        terminal,
        guard: crate::browser_auth::live::CancelOnDrop(cancellation.clone()),
        generation: generation.to_owned(),
        gate: crate::browser_auth::live::DeliveryGate {
            entry,
            shared: Arc::clone(&f.service.0),
            stop: f.tasks.as_ref().unwrap().0.browser_policy().2,
            cancellation: cancellation.clone(),
            deadline: Instant::now() + Duration::from_secs(30),
        },
        maximum: 65536,
    };
    let queued_response =
        axum::response::Sse::new(crate::browser_auth::live::subscriber_stream(queued))
            .into_response();
    let logout = f
        .client
        .post(format!("{}/auth/logout", f.origin))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    let bytes = axum::body::to_bytes(queued_response.into_body(), 65536)
        .await
        .unwrap();
    let denied = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(denied.contains("permission_denied"));
    assert!(!denied.contains("queued session payload"));
    assert!(!denied.contains("event: opened"));
    assert!(cancellation.is_cancelled());

    assert_eq!(
        call(&f, &cookie, reqwest::Method::GET, "/api/live/sessions")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    f.finish_tasks().await;
}
#[tokio::test]
async fn live_http_read_knowledge_and_mutation_are_distinct_from_memory_and_cookie_identity() {
    for (permissions, list_status) in [
        (
            vec![Permission::ReadUser, Permission::Read(Module::Memory)],
            StatusCode::FORBIDDEN,
        ),
        (
            vec![Permission::ReadUser, Permission::Read(Module::Knowledge)],
            StatusCode::OK,
        ),
    ] {
        let (_temp, f) = live_fixture(&permissions).await;
        let cookie = f.login().await;
        let response = call(&f, &cookie, reqwest::Method::GET, "/api/live/sessions").await;
        let status = response.status();
        let body = response.text().await.unwrap();
        assert_eq!(status, list_status, "sanitized Live response: {body}");
        assert_eq!(
            call(
                &f,
                &cookie,
                reqwest::Method::POST,
                "/api/live/sessions/absent/cancel"
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
        f.finish_tasks().await;
    }
}

async fn record_live_turn(f: &Fixture, turn: &str) {
    let (server, _) = f.tasks.as_ref().unwrap();
    let entry = f
        .service
        .0
        .sessions
        .lock()
        .await
        .values()
        .next()
        .cloned()
        .unwrap();
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        server.browser_policy().0,
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let shared = &f.service.0;
    let credentials = shared
        .credentials(&entry, original.snapshot())
        .await
        .unwrap_or_else(|_| panic!("real session credentials unavailable"));
    let token = zeroize::Zeroizing::new(credentials.access_token().to_owned());
    drop(credentials);
    let flow = shared
        .flow(entry.provider)
        .unwrap_or_else(|_| panic!("real OAuth flow unavailable"));
    let (provider, verifier) = &shared.providers[entry.provider];
    let caller = flow
        .verify_access(
            &entry.registered,
            &token,
            BrokerRequestContext::new(original.snapshot()),
        )
        .await
        .unwrap();
    if let Err(error) = server.record_live_browser_test_turn(
        (&caller, verifier, provider),
        &original,
        "web-history",
        turn,
    ) {
        panic!(
            "real USER history write failed with status {}",
            error.into_response().status()
        );
    }
}

#[tokio::test]
async fn live_sse_same_subscription_delivers_two_turns_beyond_rpc_budget() {
    let (_temp, f) = live_fixture(&[
        Permission::ReadUser,
        Permission::MutateUser,
        Permission::Read(Module::Knowledge),
        Permission::MutateModule(Module::Knowledge),
    ])
    .await;
    let cookie = f.login().await;
    seed_live(&f).await;
    let rpc_budget = f.tasks.as_ref().unwrap().0.browser_policy().1;
    let subscriber = call(
        &f,
        &cookie,
        reqwest::Method::GET,
        "/api/live/sessions/web-history/events",
    )
    .await;
    assert_eq!(subscriber.status(), StatusCode::OK);
    let opened_at = Instant::now();
    record_live_turn(&f, "first").await;
    // Cross the actual finite RPC budget once; it is not the lifetime of this subscription.
    tokio::time::sleep_until((opened_at + rpc_budget).into()).await;
    record_live_turn(&f, "second").await;
    let closed = call(
        &f,
        &cookie,
        reqwest::Method::POST,
        "/api/live/sessions/web-history/close",
    )
    .await;
    assert_eq!(closed.status(), StatusCode::OK);
    let body = tokio::time::timeout(Duration::from_secs(5), subscriber.text())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(body.matches("event: opened").count(), 1);
    assert_eq!(body.matches("event: closed").count(), 1);
    assert!(
        !body.contains("event: error"),
        "subscription failed: {body}"
    );
    let events: Vec<Value> = body
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .filter(|value| value.get("sequence").is_some())
        .collect();
    let sequences: Vec<u64> = events
        .iter()
        .map(|value| value["sequence"].as_str().unwrap().parse().unwrap())
        .collect();
    assert!(sequences.windows(2).all(|pair| pair[0] + 1 == pair[1]));
    for turn in ["first", "second"] {
        assert_eq!(
            events
                .iter()
                .filter(|event| event["turn_id"] == turn && event["data"]["type"] == "turn_done")
                .count(),
            1
        );
        assert!(
            events
                .iter()
                .any(|event| event["turn_id"] == turn && event["data"]["type"] == "text")
        );
    }
    let first = events
        .iter()
        .position(|event| event["turn_id"] == "first")
        .unwrap();
    let second = events
        .iter()
        .position(|event| event["turn_id"] == "second")
        .unwrap();
    assert!(first < second);
    f.finish_tasks().await;
}

#[tokio::test]
async fn live_sse_queued_payload_obeys_natural_expiry_and_actual_cancellation() {
    let (_temp, f) =
        live_fixture(&[Permission::ReadUser, Permission::Read(Module::Knowledge)]).await;
    let _cookie = f.login().await;
    let entry = f
        .service
        .0
        .sessions
        .lock()
        .await
        .values()
        .next()
        .cloned()
        .unwrap();
    for expired in [false, true] {
        let cancellation = Cancellation::default();
        cancellation.cancel();
        let (_send, progress) = tokio::sync::mpsc::channel(1);
        let (_done, terminal) = oneshot::channel();
        let state = crate::browser_auth::live::Subscriber {
            first: Some(
                axum::response::sse::Event::default()
                    .event("opened")
                    .data("queued private session payload"),
            ),
            progress,
            terminal,
            guard: crate::browser_auth::live::CancelOnDrop(cancellation.clone()),
            generation: "fixture-generation".into(),
            gate: crate::browser_auth::live::DeliveryGate {
                entry: Arc::clone(&entry),
                shared: Arc::clone(&f.service.0),
                stop: f.tasks.as_ref().unwrap().0.browser_policy().2,
                cancellation,
                deadline: if expired {
                    Instant::now()
                } else {
                    entry.expires
                },
            },
            maximum: 65536,
        };
        let response =
            axum::response::Sse::new(crate::browser_auth::live::subscriber_stream(state))
                .into_response();
        let bytes = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(body.contains(if expired { "deadline" } else { "cancelled" }));
        if expired {
            assert!(!body.contains("cancelled"));
        }
        assert!(!body.contains("queued private session payload"));
        assert!(!body.contains("event: opened"));
    }
    f.finish_tasks().await;
}
