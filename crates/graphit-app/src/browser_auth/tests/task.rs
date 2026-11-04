use super::*;
use crate::task_server::{
    TaskServer,
    tests::{browser_fixture, browser_provider, browser_selection, reopen_browser, seed_browser},
};
use graphit_auth::broker::project_access::{BrokerProjectAccess, ProjectAccessPolicy};

pub(super) fn backend(endpoint: &str) -> BrokerProjectAccess {
    BrokerProjectAccess::new(
        ProjectAccessPolicy {
            endpoint,
            network: ClientPolicy {
                origin: endpoint,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[],
            },
        },
        NetworkLimits {
            url_bytes: 8192,
            request_body_bytes: 8192,
            response_body_bytes: 32768,
            max_in_flight: 8,
            header_bytes: 16384,
            header_count: 32,
            max_idle_per_host: 2,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 2,
            max_certificate_bytes: 8192,
        },
        JsonLimits {
            bytes: 32768,
            nodes: 256,
            depth: 16,
            scalar_bytes: 8192,
            collection_entries: 64,
        },
    )
    .unwrap()
}
async fn seed(server: &TaskServer, raw: &str) -> (Value, Value, Value) {
    let session=seed_browser(server,raw,"create_session",json!({"idempotency_key":"browser-session","title":"Browser session","description":"Persisted browser corpus","strategy":"native"})).await;
    let task=seed_browser(server,raw,"create_task",json!({"idempotency_key":"browser-task","session_id":session["id"],"title":"Browser calendar","description":"Persisted scheduled task","task_type":"TASK","priority":1,"schedule":{"start_date":"2028-02-28","end_date":"2028-03-01","due_date":"2028-03-02"},"checks":[{"kind":"acceptance","text":"Native browser read"},{"kind":"test","text":"Validate native browser read"}]})).await;
    let dependent=seed_browser(server,raw,"create_task",json!({"idempotency_key":"browser-dependent","session_id":session["id"],"depends_on":[task["specification"]["id"]],"title":"Browser dependent","description":"Persisted dependency","task_type":"TASK","priority":2,"checks":[{"kind":"acceptance","text":"Native dependency"},{"kind":"test","text":"Validate native dependency"}]})).await;
    let claim = seed_browser(
        server,
        raw,
        "claim_task",
        json!({"id":task["specification"]["id"],"lease_ms":60000}),
    )
    .await;
    let comment = seed_browser(server, raw, "add_task_comment", json!({"target":{"id":task["specification"]["id"],"claim":{"token":claim["token"],"expected_revision":claim["record"]["specification"]["revision"]}},"lease_ms":60000,"idempotency_key":"browser-decision","kind":"decision","body":"Use the **native calendar**."})).await;
    seed_browser(server, raw, "set_task_flag", json!({"target":{"id":task["specification"]["id"],"claim":{"token":claim["token"],"expected_revision":comment["result"]["task"]["specification"]["revision"]}},"lease_ms":60000,"flagged":true,"reason":"Review calendar"})).await;
    (
        session["id"].clone(),
        task["specification"]["id"].clone(),
        dependent["specification"]["id"].clone(),
    )
}
impl Fixture {
    pub(super) async fn task_read(
        &self,
        cookie: &str,
        route: &str,
        arguments: Value,
    ) -> reqwest::Response {
        self.client
            .post(format!("{}/api/tasks/{route}", self.origin))
            .header(header::ORIGIN, &self.origin)
            .header(header::COOKIE, cookie)
            .json(&arguments)
            .send()
            .await
            .unwrap()
    }
    pub(super) async fn finish_tasks(&self) {
        self.service.shutdown().await.unwrap();
        if let Some((tasks, _)) = &self.tasks {
            tasks.shutdown().await.unwrap();
        }
        assert!(self.service.0.jobs.lock().await.is_empty());
    }
}
#[tokio::test]
async fn cookie_task_search_uses_full_text_comments_with_verified_project_actor() {
    let (_temp, tasks, raw) = browser_fixture();
    let (_, task_id, _) = seed(&tasks, &raw).await;
    let selection = browser_selection(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let response = fixture.task_read(&cookie, "search", json!({"selection":selection,
        "input":{"mode":"fts","text":"native calendar","filter":{},"page_size":10,"top_k":10}
    })).await;
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value = response.json().await.unwrap();
    let results = value["Published"]["results"].as_array().unwrap();
    assert!(
        results
            .iter()
            .any(|hit| hit["task"]["id"] == task_id && hit["location"].get("Comment").is_some()),
        "{value}"
    );
    fixture.issuer.project_grant.store(false, Ordering::Release);
    let denied = fixture.task_read(&cookie, "search", json!({"selection":selection,
        "input":{"mode":"fts","text":"native calendar","filter":{},"page_size":10,"top_k":10}
    })).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn cookie_reads_reopened_native_task_session_dates_dependencies_and_cursors() {
    let (_temp, tasks, raw) = browser_fixture();
    let (session, id, dependent) = seed(&tasks, &raw).await;
    tasks.shutdown().await.unwrap();
    let tasks = reopen_browser(&tasks);
    let selection = browser_selection(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    *fixture.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let cookie = fixture.login().await;
    let context = fixture.task_read(&cookie, "context", json!({})).await;
    assert_eq!(context.status(), StatusCode::OK);
    let context: Value = context.json().await.unwrap();
    assert_eq!(context, selection);
    let page = fixture
        .task_read(
            &cookie,
            "catalogue",
            json!({"selection":context,"input":{"filter":{},"page_size":1,"cursor":null}}),
        )
        .await;
    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(page.headers()[header::CACHE_CONTROL], "no-store");
    let page: Value = page.json().await.unwrap();
    assert_eq!(page["results"].as_array().unwrap().len(), 1);
    assert!(page["next_cursor"].is_object());
    let next = fixture
        .task_read(
            &cookie,
            "catalogue",
            json!({"selection":context,"input":{"filter":{},"page_size":1,"cursor":page["next_cursor"]}}),
        )
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(next["results"].as_array().unwrap().len(), 1);
    let detail = fixture
        .task_read(
            &cookie,
            "get",
            json!({"selection":context,"input":{"id":id}}),
        )
        .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail: Value = detail.json().await.unwrap();
    assert_eq!(
        detail["task"]["specification"]["schedule"]["due_date"],
        "2028-03-02"
    );
    assert!(detail["history"].is_null());
    assert!(detail["comments"].is_null());
    let project_calls_before_comments = fixture.issuer.project_calls.load(Ordering::SeqCst);
    let comments = fixture
        .task_read(
            &cookie,
            "get",
            json!({"selection":context,"input":{"id":id,"comments":{"page_size":20,"cursor":detail["comments_start_cursor"]}}}),
        )
        .await;
    assert_eq!(comments.status(), StatusCode::OK);
    let comments: Value = comments.json().await.unwrap();
    assert_eq!(comments["generation"], detail["generation"]);
    assert_eq!(comments["task"], detail["task"]);
    assert!(comments["history"].is_null());
    assert_eq!(
        comments["comments"]["comments"][0]["body"],
        "Use the **native calendar**."
    );
    assert_eq!(
        fixture.issuer.project_calls.load(Ordering::SeqCst),
        project_calls_before_comments + 1
    );
    let dependency = fixture
        .task_read(&cookie, "get", json!({"input":{"id":dependent}}))
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(dependency["task"]["depends_on"], json!([id]));
    let session = fixture
        .task_read(&cookie, "session", json!({"input":{"id":session}}))
        .await;
    assert_eq!(session.status(), StatusCode::OK);
    let session: Value = session.json().await.unwrap();
    assert!(session["history"].is_null());
    assert!(session.get("events").is_none());
    assert_eq!(session["session"]["input"]["title"], "Browser session");
    let history = fixture
        .task_read(&cookie, "session", json!({"input":{"id":session["session"]["id"],"history":{"page_size":2,"cursor":session["history_start_cursor"]}}}))
        .await;
    assert_eq!(history.status(), StatusCode::OK);
    assert_eq!(history.headers()[header::CACHE_CONTROL], "no-store");
    let history: Value = history.json().await.unwrap();
    assert_eq!(history["generation"], session["generation"]);
    assert_eq!(history["history"]["before"], session["creation"]["record"]);
    assert_eq!(history["history"]["events"], json!([]));
    assert!(history["history"]["next_cursor"].is_null());
    let view = fixture.task_read(&cookie, "view", json!({})).await;
    assert_eq!(view.status(), StatusCode::OK);
    assert!(view.text().await.unwrap().contains("Browser calendar"));
    let heads = fixture.task_read(&cookie, "session-heads", json!({})).await;
    assert_eq!(heads.status(), StatusCode::OK);
    assert_eq!(
        heads
            .json::<Value>()
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // Explicit comment and Session history pages each add a fresh project check.
    // This is a fixture count, not an admission quota.
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), 8 + 2);
    assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.issuer.keys_calls.load(Ordering::SeqCst), 1);
    let rendered = format!("{detail}{comments}");
    for field in [
        "\"access_token\":",
        "\"refresh_token\":",
        "\"id_token\":",
        "\"claim_token\":",
        "\"authorization\":",
        "\"cookie\":",
    ] {
        assert!(
            !rendered.contains(field),
            "Task response exposes a secret field"
        );
    }
    {
        let access = fixture.issuer.issued.lock().unwrap();
        assert!(!access.is_empty());
        assert!(!rendered.contains(access.as_str()));
    }
    let refresh_sequence = fixture.issuer.refresh.load(Ordering::SeqCst);
    assert!(refresh_sequence > 0);
    let refresh = format!("refresh-{refresh_sequence}");
    assert!(!rendered.contains(&refresh));
    let (_, cookie_value) = cookie
        .split_once('=')
        .expect("browser fixture has a session cookie");
    assert!(!cookie_value.is_empty());
    assert!(!rendered.contains(cookie_value));
    fixture.finish_tasks().await;
}

#[tokio::test]
#[ignore = "native Task Browser qualification requires Node and prepared UI dependencies"]
async fn task_browser_ui_reads_authorized_native_records() {
    let directory = std::env::var_os("GRAPHIT_TASK_BROWSER_UI_DIR")
        .expect("set GRAPHIT_TASK_BROWSER_UI_DIR to the prepared UI directory");
    let (_temp, tasks, raw) = browser_fixture();
    seed(&tasks, &raw).await;
    tasks.shutdown().await.unwrap();
    let tasks = reopen_browser(&tasks);
    let selection = browser_selection(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let output = tokio::time::timeout(
        Duration::from_secs(60),
        tokio::process::Command::new("npm")
            .kill_on_drop(true)
            .args([
                "test",
                "--",
                "src/components/task/TaskBrowserNative.test.tsx",
            ])
            .current_dir(directory)
            .env("npm_config_offline", "true")
            .env("GRAPHIT_QUALIFY_TASK_BROWSER", "1")
            .env("GRAPHIT_TASK_BROWSER_ORIGIN", &fixture.origin)
            .env("GRAPHIT_TASK_BROWSER_COOKIE", &cookie)
            .env(
                "GRAPHIT_TASK_BROWSER_PROJECT_ID",
                selection["project_id"].as_str().unwrap(),
            )
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    fixture.finish_tasks().await;
    assert!(
        output.status.success(),
        "Native UI consumer failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn fresh_project_denial_provider_removal_and_browser_boundaries_precede_storage() {
    let (_temp, tasks, raw) = browser_fixture();
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let empty = fixture.task_read(&cookie, "view", json!({})).await;
    assert_eq!(empty.status(), StatusCode::OK);
    assert_eq!(empty.json::<Value>().await.unwrap(), "NoCorpus");
    fixture.issuer.project_grant.store(false, Ordering::Release);
    let denied = fixture.task_read(&cookie, "view", json!({})).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        denied.json::<Value>().await.unwrap()["error"]["code"],
        "project_access_denied"
    );
    assert_eq!(
        fixture
            .task_read(&cookie, "context", json!({}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let before = fixture.issuer.project_calls.load(Ordering::SeqCst);
    for (origin, cookie) in [
        ("https://foreign.example", cookie.as_str()),
        (fixture.origin.as_str(), "graphit_session=unknown"),
    ] {
        let denied = fixture
            .client
            .post(format!("{}/api/tasks/view", fixture.origin))
            .header(header::ORIGIN, origin)
            .header(header::COOKIE, cookie)
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert!(matches!(
            denied.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ));
    }
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), before);
    let malformed = fixture
        .task_read(&cookie, "view", json!({"unexpected":true}))
        .await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    let oversized = fixture
        .task_read(
            &cookie,
            "view",
            json!({"input":{"padding":"x".repeat(65536)}}),
        )
        .await;
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let invalid = fixture
        .task_read(&cookie, "view", json!({"selection":{"project_version":""}}))
        .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    let missing = fixture
        .task_read(
            &cookie,
            "view",
            json!({"selection":{"project_version":"unretained"}}),
        )
        .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), before);
    let (tasks, raw) = fixture.tasks.as_ref().unwrap();
    browser_provider(tasks, raw, &fixture.issuer.origin, true);
    fixture.issuer.project_grant.store(true, Ordering::Release);
    let removed = fixture.task_read(&cookie, "view", json!({})).await;
    assert_eq!(removed.status(), StatusCode::FORBIDDEN);
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), before);
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn invalid_expired_and_wrong_audience_access_jwt_never_reaches_project() {
    for invalid in 0..3 {
        let (_temp, tasks, raw) = browser_fixture();
        let mut p = policy();
        p.refresh_margin = Duration::from_secs(600);
        let fixture = Fixture::with_tasks(p, Some((tasks, raw))).await;
        let cookie = fixture.login().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut claims = json!({"iss":fixture.issuer.origin,"sub":"web-subject","client_id":"web-client","token_use":"access","jti":"invalid-browser","aud":["broker-api",format!("{}/v1",fixture.issuer.origin)],"iat":now,"exp":now+300,"preferred_username":"web-user"});
        let token = match invalid {
            0 => "invalid-access-token".to_owned(),
            1 => {
                claims["exp"] = json!(now - 1);
                fixture.issuer.jwt(claims)
            }
            _ => {
                claims["aud"] = json!(["foreign", format!("{}/v1", fixture.issuer.origin)]);
                fixture.issuer.jwt(claims)
            }
        };
        *fixture.issuer.access_override.lock().unwrap() = Some(token);
        let response = fixture.task_read(&cookie, "view", json!({})).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "case {invalid}"
        );
        assert!(
            response.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
        assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            fixture.identity(&cookie).await.status(),
            StatusCode::UNAUTHORIZED
        );
        fixture.finish_tasks().await;
    }
}
#[tokio::test]
async fn concurrent_task_reads_serialize_rotation_and_shutdown_joins_cancelled_project_request() {
    let (_temp, tasks, raw) = browser_fixture();
    let mut p = policy();
    p.request_timeout = Duration::from_secs(1);
    p.refresh_margin = Duration::from_secs(600);
    let fixture = Fixture::with_tasks(p, Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    fixture.issuer.hold_refresh.store(true, Ordering::Release);
    let first = fixture.task_read(&cookie, "view", json!({}));
    let work = async {
        fixture.issuer.refresh_started.notified().await;
        assert_eq!(fixture.issuer.refresh.load(Ordering::SeqCst), 2);
        fixture.issuer.hold_refresh.store(false, Ordering::Release);
        fixture.issuer.release_refresh.notify_one();
        fixture.task_read(&cookie, "session-heads", json!({})).await
    };
    let (first, second) = tokio::join!(first, work);
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(fixture.issuer.refresh.load(Ordering::SeqCst), 3);
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), 2);
    {
        let mut jobs = fixture.service.0.jobs.lock().await;
        while let Some(result) = jobs.try_join_next() {
            result.unwrap();
        }
        // The authenticated session's expiry timer stays alive during reads.
        assert_eq!(jobs.len(), 1);
    }
    fixture.issuer.hold_project.store(true, Ordering::Release);
    use tokio::io::AsyncWriteExt;
    let url = reqwest::Url::parse(&fixture.origin).unwrap();
    let mut pending =
        tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()))
            .await
            .unwrap();
    let authority = format!("{}:{}", url.host_str().unwrap(), url.port().unwrap());
    // A pooled HTTP client's future cancellation need not close its TCP connection.
    let request = format!(
        "POST /api/tasks/view HTTP/1.1\r\nHost: {authority}\r\nOrigin: {}\r\nCookie: {}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}",
        fixture.origin, cookie
    );
    pending.write_all(request.as_bytes()).await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(5),
        fixture.issuer.project_started.notified(),
    )
    .await
    .unwrap();
    drop(pending);
    // Hyper may retain an accepted handler after EOF; its original finite deadline
    // must finish and join the request before component-wide shutdown.
    {
        let mut jobs = fixture.service.0.jobs.lock().await;
        tokio::time::timeout(Duration::from_secs(5), jobs.join_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        // Only the independent session-expiry timer remains; the read is joined.
        assert_eq!(jobs.len(), 1);
    }
    fixture.issuer.hold_project.store(false, Ordering::Release);
    fixture.issuer.release_project.notify_waiters();
    assert_eq!(
        fixture
            .task_read(&cookie, "session-heads", json!({}))
            .await
            .status(),
        StatusCode::OK
    );
    // A different request still in flight is cancelled by shutdown and remains owned until joined.
    fixture.issuer.hold_project.store(true, Ordering::Release);
    let read = fixture.task_read(&cookie, "view", json!({}));
    let stop = async {
        fixture.issuer.project_started.notified().await;
        tokio::time::timeout(Duration::from_secs(5), fixture.finish_tasks())
            .await
            .unwrap();
    };
    let (response, ()) = tokio::join!(read, stop);
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    fixture.issuer.release_project.notify_waiters();
    assert!(fixture.service.0.sessions.lock().await.is_empty());
}

pub(super) struct PublicProject {
    child: tokio::process::Child,
    input: tokio::process::ChildStdin,
    output: tokio::io::BufReader<tokio::process::ChildStdout>,
}
impl PublicProject {
    pub(super) async fn start(
        binary: &std::ffi::OsStr,
        issuer: &str,
        project_id: Value,
    ) -> (Self, Value) {
        use std::process::Stdio;
        let mut child = tokio::process::Command::new(binary)
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = tokio::io::BufReader::new(child.stdout.take().unwrap());
        let mut service = Self {
            child,
            input,
            output,
        };
        let ready=tokio::time::timeout(Duration::from_secs(180),service.command(json!({"command":"start","issuer":issuer,"api_audience":"broker-api","resource":format!("{issuer}/v1"),"client_id":"web-client","subject":"web-subject","scope":"","project_ids":[project_id],"access_ttl_seconds":600}))).await.unwrap();
        assert_eq!(ready["event"], "ready");
        (service, ready)
    }
    async fn command(&mut self, command: Value) -> Value {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        self.input
            .write_all(&serde_json::to_vec(&command).unwrap())
            .await
            .unwrap();
        self.input.write_all(b"\n").await.unwrap();
        self.input.flush().await.unwrap();
        let mut line = String::new();
        assert_ne!(self.output.read_line(&mut line).await.unwrap(), 0);
        serde_json::from_str(&line).unwrap()
    }
    async fn shutdown(mut self) {
        assert_eq!(
            self.command(json!({"command":"shutdown"})).await["ok"],
            true
        );
        drop(self.input);
        assert!(
            tokio::time::timeout(Duration::from_secs(30), self.child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
    }
}
#[tokio::test]
#[ignore = "requires the qualified public Broker PostgreSQL/TLS PROJECT fixture binary and Docker"]
async fn cookie_native_task_read_uses_public_broker_project_sql_and_web_jwt() {
    let binary = std::env::var_os("GRAPHIT_BROKER_PROJECT_FIXTURE_BIN")
        .expect("configured public Broker fixture binary");
    let (_temp, tasks, raw) = browser_fixture();
    let (_, id, _) = seed(&tasks, &raw).await;
    let mut fixture = Fixture::configured(
        policy(),
        Some((tasks, raw)),
        Some(&binary),
        #[cfg(target_os = "linux")]
        None,
    )
    .await;
    *fixture.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let cookie = fixture.login().await;
    let result = fixture
        .task_read(&cookie, "get", json!({"input":{"id":id}}))
        .await;
    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(
        result.json::<Value>().await.unwrap()["task"]["specification"]["title"],
        "Browser calendar"
    );
    let public = fixture.public_project.as_mut().unwrap();
    assert_eq!(
        public.command(json!({"command":"deny_access"})).await["ok"],
        true
    );
    assert_eq!(
        fixture
            .task_read(&cookie, "get", json!({"input":{"id":id}}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.issuer.keys_calls.load(Ordering::SeqCst), 1);
    fixture.finish_tasks().await;
    fixture.public_project.take().unwrap().shutdown().await;
}
