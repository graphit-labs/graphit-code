//! Native OAuth renewal through the actual Task and Memory record consumers.
use super::*;
use axum::response::{IntoResponse, Response};
use graphit_auth::profile::{OidcTokens, ProfileCredentials, ProfileData};
use std::collections::HashMap;

pub(super) struct RefreshState {
    calls: AtomicUsize,
    status: Mutex<StatusCode>,
    subject: Mutex<String>,
    hold: AtomicBool,
    entered: tokio::sync::Notify,
    released: tokio::sync::Notify,
}
impl Default for RefreshState {
    fn default() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            status: Mutex::new(StatusCode::OK),
            subject: Mutex::new("shared-user".into()),
            hold: AtomicBool::new(false),
            entered: tokio::sync::Notify::new(),
            released: tokio::sync::Notify::new(),
        }
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn access(state: &BrokerState, subject: &str, expired: bool) -> String {
    let now = now();
    transports::signed(
        state,
        json!({"iss":state.origin,"sub":subject,"aud":["broker-api"],
            "client_id":"graphit-cli","token_use":"access","jti":"native-refresh",
            "iat":now-120,"exp":if expired {now-60} else {now+3600},
            "preferred_username":subject}),
    )
}
fn identity(state: &BrokerState, subject: &str) -> String {
    let now = now();
    transports::signed(
        state,
        json!({"iss":state.origin,"sub":subject,"aud":"graphit-cli",
            "iat":now-1,"exp":now+3600,"preferred_username":subject}),
    )
}

/// Keep existing Web authorization-code consumers on their original route.
/// Native refresh uses this same signed OIDC issuer and actual HTTP endpoint.
pub(super) async fn tokens(
    AxumState(state): AxumState<Arc<BrokerState>>,
    body: String,
) -> Response {
    let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    if form.get("grant_type").map(String::as_str) != Some("refresh_token") {
        return transports::tokens(AxumState(state), body)
            .await
            .into_response();
    }
    assert_eq!(form["client_id"], "graphit-cli");
    assert!(matches!(
        form["refresh_token"].as_str(),
        "native-refresh-original" | "native-refresh-rotated"
    ));
    let refresh = &state.native_refresh;
    refresh.calls.fetch_add(1, Ordering::SeqCst);
    if refresh.hold.load(Ordering::SeqCst) {
        refresh.entered.notify_one();
        refresh.released.notified().await;
    }
    let status = *refresh.status.lock().unwrap();
    if status != StatusCode::OK {
        return (status, Json(json!({"error":"invalid_grant"}))).into_response();
    }
    let subject = refresh.subject.lock().unwrap().clone();
    Json(json!({"access_token":access(&state,&subject,false),
        "id_token":identity(&state,&subject),"refresh_token":"native-refresh-rotated",
        "token_type":"Bearer","expires_in":3600}))
    .into_response()
}

fn disabled_s3() -> S3Fixture {
    // A valid Broker descriptor with no S3 service never consumes these fields.
    S3Fixture {
        endpoint: "http://127.0.0.1:9".into(),
        bucket: "disabled".into(),
        region: "us-east-1".into(),
        access_key_id: String::new(),
        secret_access_key: String::new(),
        session_token: String::new(),
        _expires_at: String::new(),
    }
}
fn seed_native(client: &Client, broker: &Broker, expired: bool) -> u64 {
    let cancellation = Cancellation::default();
    let request = RequestContext::new(
        client.server.0.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            client.server.0.auth.installation().live.clone(),
            AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
            request.snapshot(),
        )
        .unwrap(),
        profile_limits(),
        &request,
    )
    .unwrap();
    let result = store
        .mutate(store.load(&request).unwrap(), &request, |doc, context| {
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("broker").unwrap().revision(),
                profile_revision: Some(doc.profile("native").unwrap().revision()),
            };
            doc.replace_profile_data(
                ProfileData {
                    name: "native".into(),
                    provider: "broker".into(),
                    username: "shared-user".into(),
                    organization: None,
                    teams: Vec::new(),
                    credentials: ProfileCredentials::Broker {
                        issuer: broker.state.origin.clone(),
                        subject: "shared-user".into(),
                        tokens: OidcTokens {
                            access_token: Secret::new(SecretString::from(access(
                                &broker.state,
                                "shared-user",
                                expired,
                            ))),
                            refresh_token: Some(Secret::new(SecretString::from(
                                "native-refresh-original",
                            ))),
                            id_token: Secret::new(SecretString::from(identity(
                                &broker.state,
                                "shared-user",
                            ))),
                            // Code OAuth tokens use UTC microseconds; Device tokens use seconds.
                            expires_at: i64::try_from(if expired {
                                now() - 60
                            } else {
                                now() + 3600
                            })
                            .unwrap()
                            .checked_mul(1_000_000)
                            .unwrap(),
                        },
                        s3_disabled: !broker.state.storage_enabled.load(Ordering::SeqCst),
                    },
                },
                expected,
                now() as i64,
                context,
            )
        })
        .unwrap();
    assert!(result.failure().is_none() && result.cleanup_failure().is_none());
    profile_identity(client).0
}
fn profile_identity(client: &Client) -> (u64, String) {
    let cancellation = Cancellation::default();
    let request = RequestContext::new(
        client.server.0.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            client.server.0.auth.installation().live.clone(),
            AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
            request.snapshot(),
        )
        .unwrap(),
        profile_limits(),
        &request,
    )
    .unwrap();
    let snapshot = store.load(&request).unwrap();
    let profile = snapshot.document().profile("native").unwrap();
    let ProfileCredentials::Broker { subject, .. } = &profile.data().unwrap().credentials else {
        panic!("native Code profile retained")
    };
    (profile.token_revision(), subject.clone())
}
fn session_input(key: &str) -> Value {
    json!({"idempotency_key":key,"title":"Native OAuth records",
        "description":"Real consumers retain the original Native subject",
        "strategy":"Refresh only an expired Code credential"})
}
async fn records_refresh(s3: Option<S3Fixture>) {
    let remote = s3.is_some();
    let broker = Broker::start(s3.unwrap_or_else(disabled_s3)).await;
    broker.state.storage_enabled.store(remote, Ordering::SeqCst);
    let client = Client::new(&broker, None);
    let before = seed_native(&client, &broker, true);
    broker
        .state
        .native_refresh
        .hold
        .store(true, Ordering::SeqCst);
    let operations = async {
        tokio::join!(
            client.task("session_create", session_input("refresh-task"), true),
            client.memory("memory_create", memory_input("refresh-memory"), 0)
        )
    };
    tokio::pin!(operations);
    tokio::select! {
        () = broker.state.native_refresh.entered.notified() => (),
        _ = &mut operations => panic!("expired Native consumers must await the real OAuth response"),
    }
    assert_eq!(broker.state.native_refresh.calls.load(Ordering::SeqCst), 1);
    broker
        .state
        .native_refresh
        .hold
        .store(false, Ordering::SeqCst);
    broker.state.native_refresh.released.notify_one();
    let (task, memory) = tokio::time::timeout(Duration::from_secs(30), operations)
        .await
        .expect("concurrent record consumers complete after one refresh");
    let session = task_value(task);
    let memory = memory.unwrap_or_else(|error| panic!("Native Memory consumer failed: {error}"));
    let session_id = session["result"]["id"].clone();
    let memory_id = memory["native"]["id"].clone();
    assert!(session_id.is_string() && memory_id.is_string());
    assert_eq!(
        profile_identity(&client),
        (before + 1, "shared-user".into())
    );
    assert_eq!(broker.state.native_refresh.calls.load(Ordering::SeqCst), 1);
    let read_task = task_value(
        client
            .task("session_get", json!({"id":session_id}), false)
            .await,
    );
    assert_eq!(
        read_task["result"]["session"]["input"]["title"],
        "Native OAuth records"
    );
    assert_eq!(
        client
            .memory("memory_get", json!({"id":memory_id}), 0)
            .await
            .unwrap()["native"]["memory"]["id"],
        memory_id
    );
    assert_eq!(
        broker.state.native_refresh.calls.load(Ordering::SeqCst),
        1,
        "a valid refreshed access token must not call OAuth again"
    );
    if remote {
        assert!(broker.state.calls.load(Ordering::SeqCst) >= 2);
        client.assert_no_local_tables();
    } else {
        assert_eq!(broker.state.calls.load(Ordering::SeqCst), 0);
    }
    client.server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_expired_records_refresh_once_and_valid_access_is_reused_with_s3_disabled() {
    records_refresh(None).await;
}

#[tokio::test]
async fn continuous_task_embedding_scan_refreshes_expired_native_without_expiring_the_corpus() {
    use graphit_records::task_store::{
        NativeTaskStore, TASK_DOCUMENT_RECIPE, TaskEmbeddingCorpusRead, TaskEmbeddingIdentity,
    };
    use std::num::{NonZeroU32, NonZeroUsize};

    let broker = Broker::start(disabled_s3()).await;
    broker.state.storage_enabled.store(false, Ordering::SeqCst);
    let client = Client::new(&broker, None);
    seed_native(&client, &broker, false);
    let session = task_value(
        client
            .task("session_create", session_input("background-refresh"), true)
            .await,
    );
    task_value(client.task("create", json!({
        "idempotency_key":"background-task", "session_id":session["result"]["id"],
        "title":"Background embeddings", "description":"Retain the current Task document during Native refresh",
        "task_type":"TASK", "priority":1,
        "checks":[{"kind":"acceptance","text":"Read the existing document"},{"kind":"test","text":"Refresh a continuous embedding scan"}]
    }), true).await);
    let before = seed_native(&client, &broker, true);
    let policy = &client.server.0.policy;
    let cancellation = Cancellation::default();
    let request = RequestContext::service(policy.content, &cancellation).unwrap();
    let snapshot = client
        .server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("selected project"));
    let (paths, id, version) = &snapshot.projects[0];
    let scope =
        CapturedPublicationScope::project(paths, *id, policy.scope, request.snapshot()).unwrap();
    let binding = scope.binding().unwrap();
    let issuer = client.server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(&client.raw, &request).unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let read = authority
        .admit_authenticated_read(
            &caller,
            &request,
            ScopeInput {
                installation: client.server.0.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &scope,
                        record: &binding,
                    },
                    version,
                }),
                providers: &[],
                binding: policy.binding,
                global_version: "",
            },
            ProjectReadModule::Task,
        )
        .unwrap();
    let identity = TaskEmbeddingIdentity::new(
        [19; 32],
        NonZeroU32::new(2).unwrap(),
        TASK_DOCUMENT_RECIPE.into(),
    )
    .unwrap();
    let captured =
        NativeTaskStore::capture_embeddings(request.snapshot(), &read, identity, policy.read)
            .await
            .unwrap();
    let TaskEmbeddingCorpusRead::Published(mut corpus) = captured else {
        panic!("existing Task corpus retained")
    };
    assert!(
        corpus
            .next_documents(request.snapshot(), &read, NonZeroUsize::new(4).unwrap())
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        profile_identity(&client),
        (before + 1, "shared-user".into())
    );
    assert_eq!(broker.state.native_refresh.calls.load(Ordering::SeqCst), 1);
    assert!(
        request.snapshot().deadline().is_none(),
        "renewal does not expire the corpus job"
    );
    cancellation.cancel();
    assert!(
        corpus
            .next_documents(request.snapshot(), &read, NonZeroUsize::MIN)
            .await
            .is_err()
    );
    client.server.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn native_expired_records_refresh_once_before_real_s3_consumers() {
    records_refresh(Some(S3Fixture::load())).await;
}

#[tokio::test]
async fn native_refresh_rejection_does_not_create_local_task_or_memory_tables() {
    let broker = Broker::start(disabled_s3()).await;
    broker.state.storage_enabled.store(false, Ordering::SeqCst);
    *broker.state.native_refresh.status.lock().unwrap() = StatusCode::UNAUTHORIZED;
    let client = Client::new(&broker, None);
    let before = seed_native(&client, &broker, true);
    let task = client
        .task("session_create", session_input("denied-refresh"), true)
        .await;
    assert!(
        task.is_err(),
        "failed OAuth must not publish a Task locally"
    );
    assert!(
        client
            .memory("memory_create", memory_input("denied-refresh-memory"), 0)
            .await
            .is_err()
    );
    assert_eq!(profile_identity(&client), (before, "shared-user".into()));
    assert_eq!(
        broker.state.native_refresh.calls.load(Ordering::SeqCst),
        1,
        "the exact rejected rotating token epoch must not be replayed"
    );
    client.assert_no_local_tables();
    seed_native(&client, &broker, false);
    task_value(
        client
            .task("session_create", session_input("fresh-native-login"), true)
            .await,
    );
    assert!(
        client
            .memory("memory_create", memory_input("fresh-native-memory"), 0)
            .await
            .is_ok()
    );
    assert_eq!(
        broker.state.native_refresh.calls.load(Ordering::SeqCst),
        1,
        "a fresh valid Native token epoch is not blocked by the earlier terminal result"
    );
    client.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn native_refresh_identity_change_is_rejected_before_record_publication() {
    let broker = Broker::start(disabled_s3()).await;
    broker.state.storage_enabled.store(false, Ordering::SeqCst);
    *broker.state.native_refresh.subject.lock().unwrap() = "other-user".into();
    let client = Client::new(&broker, None);
    let before = seed_native(&client, &broker, true);
    assert!(
        client
            .task("session_create", session_input("identity-refresh"), true)
            .await
            .is_err()
    );
    assert_eq!(broker.state.native_refresh.calls.load(Ordering::SeqCst), 1);
    assert_eq!(profile_identity(&client), (before, "shared-user".into()));
    client.assert_no_local_tables();
    client.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn native_refresh_observes_the_task_original_cancellation() {
    let broker = Broker::start(disabled_s3()).await;
    broker.state.storage_enabled.store(false, Ordering::SeqCst);
    broker
        .state
        .native_refresh
        .hold
        .store(true, Ordering::SeqCst);
    let client = Client::new(&broker, None);
    let before = seed_native(&client, &broker, true);
    let cancellation = CancellationToken::new();
    let operation = client.task_at_cancel(
        "session_create",
        session_input("cancel-refresh"),
        true,
        0,
        cancellation.clone(),
    );
    tokio::pin!(operation);
    tokio::select! {
        () = broker.state.native_refresh.entered.notified() => (),
        _ = &mut operation => panic!("real OAuth request is held before cancellation"),
    }
    cancellation.cancel();
    let failure = tokio::time::timeout(Duration::from_secs(5), operation)
        .await
        .unwrap()
        .expect_err("cancelled OAuth consumer fails explicitly");
    assert!(matches!(
        failure.code,
        "cancelled" | "request_cancelled_or_expired"
    ));
    broker
        .state
        .native_refresh
        .hold
        .store(false, Ordering::SeqCst);
    broker.state.native_refresh.released.notify_waiters();
    assert_eq!(broker.state.native_refresh.calls.load(Ordering::SeqCst), 1);
    assert_eq!(profile_identity(&client), (before, "shared-user".into()));
    client.assert_no_local_tables();
    tokio::time::timeout(Duration::from_secs(5), client.server.shutdown())
        .await
        .unwrap()
        .unwrap();
}

async fn user_refresh(s3: Option<S3Fixture>) {
    let remote = s3.is_some();
    let broker = Broker::start(s3.unwrap_or_else(disabled_s3)).await;
    broker.state.storage_enabled.store(remote, Ordering::SeqCst);
    let client = Client::new(&broker, None);
    let before = seed_native(&client, &broker, true);
    let created = client
        .user_storage("user_memory_create", memory_input("refresh-user"), remote)
        .await
        .unwrap_or_else(|error| panic!("Native USER pre-admission refresh failed: {error}"));
    let id = created["native"]["id"].clone();
    assert!(id.is_string());
    assert_eq!(
        profile_identity(&client),
        (before + 1, "shared-user".into())
    );
    assert_eq!(broker.state.native_refresh.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        client
            .user_storage("user_memory_get", json!({"id":id}), remote)
            .await
            .unwrap()["native"]["memory"]["id"],
        id
    );
    assert_eq!(broker.state.native_refresh.calls.load(Ordering::SeqCst), 1);
    if remote {
        assert!(broker.state.requests.lock().unwrap().iter().any(|request| {
            request["scope"] == "user"
                && request["module"] == "memory"
                && request["installation_id"].is_string()
        }));
        client.assert_no_local_tables();
    } else {
        assert_eq!(broker.state.calls.load(Ordering::SeqCst), 0);
    }
    client.server.shutdown().await.unwrap();
}
#[tokio::test]
async fn native_user_memory_refreshes_before_authentication_with_s3_disabled() {
    user_refresh(None).await;
}
#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn native_user_memory_refreshes_before_authentication_and_real_s3() {
    user_refresh(Some(S3Fixture::load())).await;
}

#[tokio::test]
async fn native_refresh_observes_the_task_original_deadline() {
    let broker = Broker::start(disabled_s3()).await;
    broker.state.storage_enabled.store(false, Ordering::SeqCst);
    broker
        .state
        .native_refresh
        .hold
        .store(true, Ordering::SeqCst);
    let mut client = Client::new(&broker, None);
    client.timeout(Duration::from_millis(300));
    let before = seed_native(&client, &broker, true);
    let operation = client.task("session_create", session_input("deadline-refresh"), true);
    tokio::pin!(operation);
    tokio::select! {
        () = broker.state.native_refresh.entered.notified() => (),
        _ = &mut operation => panic!("real OAuth request is held before the original deadline"),
    }
    let failure = tokio::time::timeout(Duration::from_secs(5), operation)
        .await
        .unwrap()
        .expect_err("expired OAuth consumer fails explicitly");
    assert!(matches!(
        failure.code,
        "deadline" | "request_cancelled_or_expired"
    ));
    broker
        .state
        .native_refresh
        .hold
        .store(false, Ordering::SeqCst);
    broker.state.native_refresh.released.notify_waiters();
    assert_eq!(broker.state.native_refresh.calls.load(Ordering::SeqCst), 1);
    assert_eq!(profile_identity(&client), (before, "shared-user".into()));
    client.assert_no_local_tables();
    tokio::time::timeout(Duration::from_secs(5), client.server.shutdown())
        .await
        .unwrap()
        .unwrap();
}
