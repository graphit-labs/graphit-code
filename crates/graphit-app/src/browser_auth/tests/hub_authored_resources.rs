//! Genuine Web PROJECT Resources discovery and capture, without remote installation grants.
use super::*;
use futures_util::FutureExt;
use graphit_auth::Permission;
use graphit_core::{brand::Brand, module_settings::Module};
use graphit_hub::{ArtifactId, ArtifactType, ConcreteVersion, NativeBinding, SourceRef};
use hub_remote_catalog::request;

const LIST: &str = "/api/hub/authored/list";
const REVIEW: &str = "/api/hub/authored/review";
const CONTENT: &str = "/api/hub/authored/content";
const ID: &str = "fixture/authored-source";
const TEXT: &str = "Actual authored PROJECT source\n";
type Fixture = crate::source_server::test_fixture::Fixture;
type Reader = tokio::task::JoinHandle<reqwest::Response>;

fn resource(
    fixture: &Fixture,
    project: usize,
    kind: ArtifactType,
    id: &str,
    version: &str,
) -> std::path::PathBuf {
    let limits = fixture.server.policy().native_install.hub.projection();
    let (paths, project_id, _) = &fixture.projects[project];
    let source = checked(
        SourceRef::resources(
            paths,
            *project_id,
            kind,
            checked(ArtifactId::new(id, limits), "authored identity"),
            checked(
                ConcreteVersion::new(version, limits),
                "literal source version",
            ),
            limits,
        ),
        "maintained Resources source",
    );
    let directory = source.directory().as_path().to_owned();
    checked(
        std::fs::create_dir_all(&directory),
        "actual Resources directory",
    );
    checked(
        std::fs::write(directory.join("SKILL.md"), TEXT),
        "actual source bytes",
    );
    directory
}

fn selection(fixture: &Fixture, project: usize) -> Value {
    json!({"project_id":fixture.projects[project].1,
        "project_version":fixture.projects[project].2.version()})
}

fn exact(fixture: &Fixture, project: usize) -> Value {
    let mut input = selection(fixture, project);
    input["artifact_type"] = json!("skill");
    input["id"] = json!(ID);
    input["version"] = json!("release/one");
    input
}

async fn response(
    host: &WebHost,
    cookie: Option<&str>,
    route: &str,
    body: &str,
    status: StatusCode,
    code: Option<&str>,
) -> Value {
    let response = request(host, cookie, route, body).await;
    assert_eq!(response.status(), status, "authored finite HTTP status");
    assert!(
        response.headers()[header::CACHE_CONTROL]
            .to_str()
            .expect("authored cache policy")
            .contains("no-store")
    );
    let bytes = checked(response.bytes().await, "bounded authored response");
    let decoded: Value = checked(serde_json::from_slice(&bytes), "authored public envelope");
    if let Some(code) = code {
        assert_eq!(decoded["error"]["code"], code);
        assert!(
            decoded.get("content").is_none(),
            "failure exposes no source payload"
        );
        for forbidden in [
            "project_dir",
            "source_dir",
            "credentials",
            "bearer",
            "proof",
        ] {
            assert!(
                !String::from_utf8_lossy(&bytes).contains(forbidden),
                "public error stays sanitized"
            );
        }
    }
    decoded
}

async fn page(host: &WebHost, cookie: &str, route: &str, input: &Value) -> Value {
    response(
        host,
        Some(cookie),
        route,
        &input.to_string(),
        StatusCode::OK,
        None,
    )
    .await
}

async fn fail(
    host: &WebHost,
    cookie: &str,
    route: &str,
    input: &Value,
    status: StatusCode,
    code: &str,
) {
    response(
        host,
        Some(cookie),
        route,
        &input.to_string(),
        status,
        Some(code),
    )
    .await;
}

async fn discovery(fixture: &Fixture, host: &WebHost, cookie: &str, second: &str) {
    let mut query = selection(fixture, 0);
    query["limit"] = json!(2);
    let first = page(host, cookie, LIST, &query).await;
    assert_eq!(first["status"], "ok");
    assert_eq!(first["project_id"], query["project_id"]);
    assert_eq!(first["entries"].as_array().unwrap().len(), 2);
    assert!(
        first["next_cursor"].is_string(),
        "real lookahead continuation"
    );
    let cursor = first["next_cursor"].clone();
    let mut observed = first["entries"].as_array().unwrap().clone();
    // A current inventory may grow after a page. This source sorts after its boundary.
    resource(
        fixture,
        0,
        ArtifactType::Skill,
        "zz/current-inventory",
        "release/new",
    );
    query["cursor"] = cursor.clone();
    for _ in 0..8 {
        let current = page(host, cookie, LIST, &query).await;
        observed.extend(current["entries"].as_array().unwrap().iter().cloned());
        if current["next_cursor"].is_null() {
            break;
        }
        query["cursor"] = current["next_cursor"].clone();
    }
    assert_eq!(
        observed.len(),
        5,
        "all maintained identities including new source reach EOF"
    );
    let identities: Vec<_> = observed
        .iter()
        .map(|entry| {
            assert!(
                entry.as_object().unwrap().contains_key("name") && entry["name"].is_null(),
                "explicitly absent descriptor metadata"
            );
            (
                entry["artifact_type"].as_str().unwrap(),
                entry["id"].as_str().unwrap(),
                entry["version"].as_str().unwrap(),
            )
        })
        .collect();
    assert!(
        identities.windows(2).all(|pair| pair[0] < pair[1]),
        "strict lexical tuple order without repeated pages"
    );
    assert!(identities.contains(&("skill", "zz/current-inventory", "release/new")));
    let mut filtered = selection(fixture, 0);
    filtered["limit"] = json!(8);
    filtered["artifact_type"] = json!("skill");
    filtered["id"] = json!(ID);
    filtered["version"] = json!("release/two");
    let literal = page(host, cookie, LIST, &filtered).await;
    assert_eq!(literal["entries"].as_array().unwrap().len(), 1);
    assert_eq!(literal["entries"][0]["version"], "release/two");
    assert!(literal["next_cursor"].is_null());
    filtered["version"] = json!("latest");
    assert!(
        page(host, cookie, LIST, &filtered).await["entries"]
            .as_array()
            .unwrap()
            .is_empty(),
        "literal version never resolves latest"
    );
    for (field, value) in [
        ("project_id", json!(fixture.projects[1].1)),
        ("project_version", json!("other-version")),
        ("id", json!("other-id")),
    ] {
        let mut invalid = selection(fixture, 0);
        invalid["limit"] = json!(2);
        invalid["cursor"] = cursor.clone();
        invalid[field] = value;
        fail(
            host,
            cookie,
            LIST,
            &invalid,
            StatusCode::BAD_REQUEST,
            "invalid_arguments",
        )
        .await;
    }
    let mut second_query = selection(fixture, 1);
    second_query["limit"] = json!(8);
    let own = page(host, second, LIST, &second_query).await;
    assert_eq!(own["entries"].as_array().unwrap().len(), 1);
    assert_eq!(own["entries"][0]["id"], "second/own-source");
    let mut first_query = selection(fixture, 0);
    first_query["limit"] = json!(8);
    fail(
        host,
        second,
        LIST,
        &first_query,
        StatusCode::FORBIDDEN,
        "authentication_or_permission_denied",
    )
    .await;
    fail(
        host,
        cookie,
        LIST,
        &second_query,
        StatusCode::FORBIDDEN,
        "authentication_or_permission_denied",
    )
    .await;
}

const LITERAL_PATH: &str = "notes:one\\part.md";
const LITERAL_TEXT: &str = "Literal Unix filename\n";

async fn inspect(fixture: &Fixture, host: &WebHost, cookie: &str) {
    let mut input = exact(fixture, 0);
    input["limit"] = json!(2);
    let mut paths = Vec::new();
    for _ in 0..8 {
        let review = page(host, cookie, REVIEW, &input).await;
        assert_eq!(review["source"], "Resources");
        assert!(
            review.as_object().unwrap().contains_key("metadata") && review["metadata"].is_null()
        );
        for entry in review["entries"].as_array().unwrap() {
            assert!(entry["bytes"].is_u64());
            assert!(matches!(entry["kind"].as_str(), Some("File" | "Directory")));
            paths.push((
                entry["path"].as_str().unwrap().to_owned(),
                entry["kind"].as_str().unwrap().to_owned(),
            ));
        }
        if review["next_offset"].is_null() {
            break;
        }
        input["offset"] = review["next_offset"].clone();
    }
    paths.sort();
    assert_eq!(
        paths,
        vec![
            ("SKILL.md".to_owned(), "File".to_owned()),
            ("binary.bin".to_owned(), "File".to_owned()),
            ("empty.txt".to_owned(), "File".to_owned()),
            (LITERAL_PATH.to_owned(), "File".to_owned()),
            ("references".to_owned(), "Directory".to_owned()),
            ("references/guide.md".to_owned(), "File".to_owned())
        ]
    );
    let mut content = exact(fixture, 0);
    content["path"] = json!("SKILL.md");
    let value = page(host, cookie, CONTENT, &content).await;
    assert_eq!(value["source"], "Resources");
    assert_eq!(
        value["content"],
        json!({"path":"SKILL.md","content":TEXT,"bytes":TEXT.len()})
    );
    content["path"] = json!("references/guide.md");
    assert_eq!(
        page(host, cookie, CONTENT, &content).await["content"]["content"],
        "Nested authored source\n"
    );
    content["path"] = json!("empty.txt");
    assert_eq!(
        page(host, cookie, CONTENT, &content).await["content"],
        json!({"path":"empty.txt","content":"","bytes":0})
    );
    content["path"] = json!(LITERAL_PATH);
    assert_eq!(
        page(host, cookie, CONTENT, &content).await["content"],
        json!({"path":LITERAL_PATH,"content":LITERAL_TEXT,"bytes":LITERAL_TEXT.len()})
    );
    content["path"] = json!("missing.txt");
    fail(
        host,
        cookie,
        CONTENT,
        &content,
        StatusCode::NOT_FOUND,
        "authored_file_not_found",
    )
    .await;
    content["path"] = json!("binary.bin");
    fail(
        host,
        cookie,
        CONTENT,
        &content,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "authored_content_not_text",
    )
    .await;
    content["path"] = json!("SKILL.md");
    content["version"] = json!("release/missing");
    fail(
        host,
        cookie,
        CONTENT,
        &content,
        StatusCode::NOT_FOUND,
        "authored_source_not_found",
    )
    .await;
    let mut review = exact(fixture, 0);
    review["limit"] = json!(8);
    review["version"] = json!("release/missing");
    fail(
        host,
        cookie,
        REVIEW,
        &review,
        StatusCode::NOT_FOUND,
        "authored_source_not_found",
    )
    .await;
}

async fn invalid_inputs(fixture: &Fixture, host: &WebHost, cookie: &str) {
    let mut content = exact(fixture, 0);
    content["path"] = json!("SKILL.md");
    let unauthenticated = response(
        host,
        None,
        CONTENT,
        &content.to_string(),
        StatusCode::UNAUTHORIZED,
        None,
    )
    .await;
    assert_eq!(unauthenticated, json!({"error":"session unavailable"}));
    for route in [LIST, REVIEW, CONTENT] {
        for body in [
            "{}",
            r#"{"project_id":null}"#,
            r#"{"project_id":"bad","project_id":"bad"}"#,
        ] {
            response(
                host,
                Some(cookie),
                route,
                body,
                StatusCode::BAD_REQUEST,
                Some("invalid_arguments"),
            )
            .await;
        }
        let oversized = request(host, Some(cookie), route, &" ".repeat(65537)).await;
        assert_eq!(
            oversized.status(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "body ceiling before source I/O"
        );
    }
    for (field, value) in [
        ("project_dir", json!("/foreign")),
        ("source_dir", json!("/foreign")),
        ("provider", json!("broker")),
        ("profile", json!("keys")),
        ("authority", json!({})),
        ("global", json!(true)),
        ("path", Value::Null),
    ] {
        let mut invalid = content.clone();
        invalid[field] = value;
        fail(
            host,
            cookie,
            CONTENT,
            &invalid,
            StatusCode::BAD_REQUEST,
            "invalid_arguments",
        )
        .await;
    }
    for path in [
        "",
        "/absolute",
        "../raw",
        "a/../raw",
        "./SKILL.md",
        "a//b",
        "\0",
    ] {
        let mut invalid = content.clone();
        invalid["path"] = json!(path);
        fail(
            host,
            cookie,
            CONTENT,
            &invalid,
            StatusCode::BAD_REQUEST,
            "invalid_arguments",
        )
        .await;
    }
    let mut list = selection(fixture, 0);
    list["limit"] = json!(1);
    for (field, value) in [
        ("artifact_type", Value::Null),
        ("id", Value::Null),
        ("version", Value::Null),
        ("cursor", Value::Null),
        ("limit", json!(0)),
        ("limit", json!(-1)),
        ("limit", json!("1")),
        ("limit", json!(u64::MAX)),
    ] {
        let mut invalid = list.clone();
        invalid[field] = value;
        fail(
            host,
            cookie,
            LIST,
            &invalid,
            StatusCode::BAD_REQUEST,
            "invalid_arguments",
        )
        .await;
    }
    let duplicate = format!("{},\"limit\":1}}", list.to_string().trim_end_matches('}'));
    response(
        host,
        Some(cookie),
        LIST,
        &duplicate,
        StatusCode::BAD_REQUEST,
        Some("invalid_arguments"),
    )
    .await;
    let mut review = exact(fixture, 0);
    review["limit"] = json!(1);
    review["offset"] = json!(u64::MAX);
    fail(
        host,
        cookie,
        REVIEW,
        &review,
        StatusCode::BAD_REQUEST,
        "invalid_arguments",
    )
    .await;
}

async fn bounds_and_links(
    fixture: &Fixture,
    host: &WebHost,
    cookie: &str,
    directory: &std::path::Path,
) {
    let scalar = fixture
        .server
        .policy()
        .native_install
        .hub
        .content()
        .max_scalar_bytes;
    checked(
        std::fs::write(
            directory.join("large.txt"),
            vec![b'x'; scalar.checked_add(1).unwrap()],
        ),
        "real oversized source file",
    );
    let mut input = exact(fixture, 0);
    input["path"] = json!("large.txt");
    fail(
        host,
        cookie,
        CONTENT,
        &input,
        StatusCode::PAYLOAD_TOO_LARGE,
        "authored_too_large",
    )
    .await;
    checked(
        std::fs::remove_file(directory.join("large.txt")),
        "remove owned large file",
    );
    #[cfg(unix)]
    {
        checked(
            std::os::unix::fs::symlink(directory.join("SKILL.md"), directory.join("linked.md")),
            "actual source file symlink",
        );
        input["path"] = json!("linked.md");
        let rejected = request(host, Some(cookie), CONTENT, &input.to_string()).await;
        assert!(
            !rejected.status().is_success(),
            "RejectFileLinks cannot expose even an in-root file link"
        );
        let value: Value = checked(rejected.json().await, "finite file-link rejection");
        assert!(value.get("content").is_none());
        checked(
            std::fs::remove_file(directory.join("linked.md")),
            "remove owned link",
        );
    }
}

fn spawn_read(host: &WebHost, cookie: &str, input: &Value) -> Reader {
    let send = host
        .client
        .post(format!("{}{CONTENT}", host.origin))
        .header(header::ORIGIN, &host.origin)
        .header(header::COOKIE, cookie)
        .header("X-Graphit-Request", "ui")
        .json(input);
    tokio::spawn(async move { checked(send.send().await, "owned authored HTTP request") })
}

async fn entered(pause: &crate::authored_resources::pause::CapturePause) {
    checked(
        tokio::time::timeout(Duration::from_secs(15), async {
            while !pause.entered() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await,
        "request reaches real Core capture before lifecycle action",
    );
}

async fn drain_completed(host: &WebHost) {
    let mut jobs = host.service.0.jobs.lock().await;
    while let Some(result) = jobs.try_join_next() {
        checked(result, "completed authored job joined");
    }
}

async fn join_read(readers: &mut Vec<Reader>) -> reqwest::Response {
    let result = checked(
        tokio::time::timeout(
            Duration::from_secs(15),
            readers.last_mut().expect("retained authored client"),
        )
        .await,
        "authored response quiesces before guard",
    );
    readers.pop();
    checked(result, "authored client joined")
}

async fn lifecycle(
    fixture: &Fixture,
    host: &WebHost,
    cookie: &str,
    sibling: &str,
    directory: &std::path::Path,
    readers: &mut Vec<Reader>,
) {
    use crate::authored_resources::pause::after_capture;
    let mut input = exact(fixture, 0);
    input["path"] = json!("SKILL.md");
    drain_completed(host).await;
    {
        let pause = after_capture(fixture.projects[0].1);
        readers.push(spawn_read(host, cookie, &input));
        entered(&pause).await;
        checked(
            std::fs::write(
                directory.join("SKILL.md"),
                "Changed after actual Core capture\n",
            ),
            "actual captured source replacement",
        );
        pause.release();
        let response = join_read(readers).await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let failure: Value = checked(response.json().await, "integrity failure envelope");
        assert_eq!(failure["error"]["code"], "authored_read_failed");
        assert!(failure.get("content").is_none());
        hub_remote_catalog::join_catalog_read_job(host).await;
    }
    checked(
        std::fs::write(directory.join("SKILL.md"), TEXT),
        "restore owned source after integrity proof",
    );
    {
        let pause = after_capture(fixture.projects[0].1);
        readers.push(spawn_read(host, cookie, &input));
        entered(&pause).await;
        let logout = checked(
            host.client
                .post(format!("{}/auth/logout", host.origin))
                .header(header::ORIGIN, &host.origin)
                .header(header::COOKIE, cookie)
                .header("X-Graphit-Request", "ui")
                .send()
                .await,
            "logout during retained source capture",
        );
        assert_eq!(logout.status(), StatusCode::NO_CONTENT);
        // The blocking reader must observe cancellation itself, before explicit release.
        let response = join_read(readers).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let failure: Value = checked(response.json().await, "logout capture failure");
        assert!(failure.get("content").is_none());
        pause.release();
    }
    // Logout cancels its session timer too, so drain actual completions before a fresh sibling read.
    drain_completed(host).await;
    assert_eq!(
        page(host, sibling, CONTENT, &input).await["content"]["content"],
        TEXT
    );
    drain_completed(host).await;
    {
        let pause = after_capture(fixture.projects[0].1);
        readers.push(spawn_read(host, sibling, &input));
        entered(&pause).await;
        let reader = readers.last_mut().unwrap();
        reader.abort();
        assert!(
            reader.await.is_err_and(|error| error.is_cancelled()),
            "client future disconnected"
        );
        readers.pop();
        hub_remote_catalog::join_catalog_read_job(host).await;
        pause.release();
    }
    assert_eq!(
        page(host, sibling, CONTENT, &input).await["content"]["content"],
        TEXT
    );
}

async fn deadline(fixture: &Fixture, host: &WebHost, cookie: &str, readers: &mut Vec<Reader>) {
    let mut input = exact(fixture, 0);
    input["path"] = json!("SKILL.md");
    drain_completed(host).await;
    let pause = crate::authored_resources::pause::after_capture(fixture.projects[0].1);
    readers.push(spawn_read(host, cookie, &input));
    entered(&pause).await;
    let response = join_read(readers).await;
    assert_eq!(
        response.status(),
        StatusCode::GATEWAY_TIMEOUT,
        "existing request deadline stops captured reader"
    );
    let failure: Value = checked(response.json().await, "deadline source failure");
    assert!(
        failure.get("content").is_none(),
        "expired admission delivers no captured bytes"
    );
    hub_remote_catalog::join_catalog_read_job(host).await;
    pause.release();
}

async fn output_bound(fixture: &Fixture, host: &WebHost, cookie: &str) {
    let mut input = exact(fixture, 0);
    input["path"] = json!("SKILL.md");
    let response = request(host, Some(cookie), CONTENT, &input.to_string()).await;
    assert_eq!(
        response.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "actual serialized source envelope obeys Browser budget"
    );
    let bytes = checked(response.bytes().await, "bounded output failure bytes");
    assert!(bytes.len() <= 256);
    let failure: Value = checked(
        serde_json::from_slice(&bytes),
        "bounded output public error",
    );
    assert!(failure.get("content").is_none());
}

async fn stop_captured_read(
    fixture: &Fixture,
    host: &WebHost,
    cookie: &str,
    readers: &mut Vec<Reader>,
) {
    let mut input = exact(fixture, 0);
    input["path"] = json!("SKILL.md");
    let pause = crate::authored_resources::pause::after_capture(fixture.projects[0].1);
    readers.push(spawn_read(host, cookie, &input));
    entered(&pause).await;
    host.service.0.stop.cancel();
    let response = join_read(readers).await;
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "Browser stop cancels its captured reader before delivery"
    );
    let failure: Value = checked(response.json().await, "stopped source error");
    assert!(failure.get("content").is_none());
    pause.release();
    // The enclosing genuine WebHost::close drains every worker and timer.
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires genuine Broker Web fixture and coordinated App emission"]
async fn real_web_authored_resources_preserve_project_authority_capture_and_lifecycle() {
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit genuine Broker fixture executable");
    let fixture = configure_hub_fixture(
        crate::source_server::test_fixture::fixture_with_web_read_providers(
            &[Permission::Read(Module::Hub)],
            &[Permission::ManageAuthProfiles],
            Brand::default(),
            NativeBinding::KiroV3,
            &["authored-first", "authored-second"],
            &[],
            (None, None),
        ),
    )
    .await;
    // DaemonAuth::start already captured these exact fixture roots before ingress.
    // No dynamic trust expansion or Runtime Configure grant is needed for this read.
    let started = std::panic::AssertUnwindSafe(BrokerProcess::start_web_projects(
        std::path::Path::new(&binary),
        &fixture.projects[0].1.to_string(),
        Some(&fixture.projects[1].1.to_string()),
        false,
        false,
    ))
    .catch_unwind()
    .await;
    let broker = match started {
        Ok(broker) => broker,
        Err(failure) => {
            let _ = fixture.server.shutdown().await;
            std::panic::resume_unwind(failure);
        }
    };
    let mut host = None;
    let mut task_owner = None;
    let mut task_directory = None;
    let mut readers = Vec::new();
    let mut baseline = None;
    let mut limited_host = None;
    let mut limited_tasks = None;
    let mut deadline_host = None;
    let mut deadline_tasks = None;
    let observed = std::panic::AssertUnwindSafe(Box::pin(async {
        let directory = resource(&fixture, 0, ArtifactType::Skill, ID, "release/one");
        checked(
            std::fs::create_dir(directory.join("references")),
            "real resource companion directory",
        );
        checked(
            std::fs::write(
                directory.join("references/guide.md"),
                "Nested authored source\n",
            ),
            "real resource companion text",
        );
        checked(
            std::fs::write(directory.join("empty.txt"), b""),
            "real empty authored file",
        );
        checked(
            std::fs::write(directory.join("binary.bin"), [0xff, 0xfe, 0x00]),
            "real binary authored file",
        );
        checked(
            std::fs::write(directory.join(LITERAL_PATH), LITERAL_TEXT),
            "real Unix colon and backslash filename",
        );
        resource(&fixture, 0, ArtifactType::Skill, ID, "release/two");
        resource(
            &fixture,
            0,
            ArtifactType::Agent,
            "agent/source",
            "release/one",
        );
        resource(&fixture, 0, ArtifactType::Ast, "ast/source", "release/one");
        resource(
            &fixture,
            1,
            ArtifactType::Skill,
            "second/own-source",
            "release/one",
        );
        let revision = crate::graph_server::tests::browser_provider(
            &fixture.server.graph_server(),
            &fixture.raw,
            wire(&broker.ready, "endpoint"),
        );
        let (directory_owner, tasks, _) =
            crate::task_server::tests::fixture_with_reference_permissions(&[], &[], None, false);
        task_directory = Some(directory_owner);
        task_owner = Some(tasks.clone());
        host = Some(
            WebHost::start(
                fixture.server.clone(),
                Some(tasks.clone()),
                &broker.ready,
                revision,
            )
            .await,
        );
        let web = host.as_ref().unwrap();
        let cookie = web.login(&broker.ready).await;
        let sibling = web.login(&broker.ready).await;
        let mut second_ready = broker.ready.clone();
        assert!(
            second_ready["web_second"].is_object(),
            "two real Web principals"
        );
        second_ready["web"] = broker.ready["web_second"].clone();
        let second = web.login(&second_ready).await;
        let first_identity: Value = checked(
            checked(
                web.client
                    .get(format!("{}/auth/session", web.origin))
                    .header(header::COOKIE, cookie.as_str())
                    .send()
                    .await,
                "first verified session",
            )
            .json()
            .await,
            "first subject",
        );
        let second_identity: Value = checked(
            checked(
                web.client
                    .get(format!("{}/auth/session", web.origin))
                    .header(header::COOKIE, second.as_str())
                    .send()
                    .await,
                "second verified session",
            )
            .json()
            .await,
            "second subject",
        );
        assert!(
            wire(&first_identity, "subject") != wire(&second_identity, "subject"),
            "independent verified Broker subjects"
        );
        baseline = Some((
            hub_file_reads::registry_bytes(&fixture.server.policy().registry.path()),
            hub_file_reads::installation_children(fixture.paths.root()),
        ));
        Box::pin(discovery(&fixture, web, &cookie, &second)).await;
        Box::pin(inspect(&fixture, web, &cookie)).await;
        Box::pin(invalid_inputs(&fixture, web, &cookie)).await;
        Box::pin(bounds_and_links(&fixture, web, &cookie, &directory)).await;
        let (mut limits, timeout, _) = tasks.browser_policy();
        limits.max_output_bytes = 256;
        let limited = tasks.references_test_policy(limits, timeout);
        limited_tasks = Some(limited.clone());
        limited_host = Some(
            WebHost::start(
                fixture.server.clone(),
                Some(limited),
                &broker.ready,
                revision,
            )
            .await,
        );
        let limited = limited_host.as_ref().unwrap();
        let limited_cookie = limited.login(&broker.ready).await;
        Box::pin(output_bound(&fixture, limited, &limited_cookie)).await;
        let (limits, _, _) = tasks.browser_policy();
        let timed = tasks.references_test_policy(limits, Duration::from_secs(3));
        deadline_tasks = Some(timed.clone());
        deadline_host = Some(
            WebHost::start(fixture.server.clone(), Some(timed), &broker.ready, revision).await,
        );
        let timed = deadline_host.as_ref().unwrap();
        let timed_cookie = timed.login(&broker.ready).await;
        Box::pin(deadline(&fixture, timed, &timed_cookie, &mut readers)).await;
        Box::pin(lifecycle(
            &fixture,
            web,
            &cookie,
            &sibling,
            &directory,
            &mut readers,
        ))
        .await;
        Box::pin(stop_captured_read(&fixture, web, &sibling, &mut readers)).await;
    }))
    .catch_unwind()
    .await;
    for reader in readers {
        reader.abort();
        let _ = reader.await;
    }
    let additional = std::panic::AssertUnwindSafe(async {
        if let Some(web) = limited_host {
            web.close().await;
        }
        if let Some(tasks) = limited_tasks {
            checked(tasks.shutdown().await, "bounded output Task owner joined");
        }
        if let Some(web) = deadline_host {
            web.close().await;
        }
        if let Some(tasks) = deadline_tasks {
            checked(tasks.shutdown().await, "deadline Task owner joined");
        }
    })
    .catch_unwind()
    .await;
    let cleanup = std::panic::AssertUnwindSafe(finish_remote_test_owners(
        observed.and(additional),
        host,
        Some(&fixture.server),
        broker,
    ))
    .catch_unwind()
    .await;
    let tasks = std::panic::AssertUnwindSafe(async {
        if let Some(tasks) = task_owner {
            checked(tasks.shutdown().await, "authored Task owner joined");
        }
    })
    .catch_unwind()
    .await;
    let effects = std::panic::AssertUnwindSafe(async {
        if let Some((registry, children)) = baseline {
            assert!(
                hub_file_reads::registry_bytes(&fixture.server.policy().registry.path())
                    == registry,
                "authored reads preserve registry exactly after shutdown"
            );
            assert!(
                hub_file_reads::installation_children(fixture.paths.root()) == children,
                "authored reads create no installation cache or pin parent"
            );
        }
    })
    .catch_unwind()
    .await;
    drop(task_directory);
    for result in [cleanup, tasks, effects] {
        if let Err(failure) = result {
            std::panic::resume_unwind(failure);
        }
    }
}
