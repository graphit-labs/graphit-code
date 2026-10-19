//! Real registered Web login, signed Broker decisions and native PROJECT reads.
use super::*;
use crate::graph_server::{GraphServer, tests::web_fixture};

async fn post(f: &Fixture, cookie: &str, route: &str, body: Value) -> reqwest::Response {
    f.client
        .post(format!("{}/api/ast/{route}", f.origin))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, cookie)
        .json(&body)
        .send()
        .await
        .unwrap()
}
fn project_settings(
    source: &crate::source_server::test_fixture::Fixture,
    settings: &[(&str, bool)],
) {
    let store = graphit_core::config::ConfigStore::project(&source.projects[0].0).unwrap();
    for (key, value) in settings {
        store
            .set(
                graphit_core::config::ConfigKey::new(key).unwrap(),
                json!(value),
                Duration::from_secs(5),
                &Cancellation::default(),
            )
            .unwrap();
    }
}
impl Fixture {
    pub(super) async fn finish_graphs(&self) {
        self.service.shutdown().await.unwrap();
        self.graphs.as_ref().unwrap().shutdown().await.unwrap();
        assert!(self.service.0.jobs.lock().await.is_empty());
    }
}
async fn open(
    graphs: &GraphServer,
    source: &mut crate::source_server::test_fixture::Fixture,
) -> Fixture {
    crate::source_server::replace_fixture_graph(source, graphs.clone());
    Fixture::with_graphs(policy(), (source.server.clone(), source.raw.clone())).await
}
#[tokio::test]
async fn cookie_ast_live_project_reads_use_read_ast_without_task_grant_or_userinfo() {
    let (mut source, graphs, destination) = web_fixture().await;
    project_settings(&source, &[("modules.task", false)]);
    let f = open(&graphs, &mut source).await;
    f.issuer.denied_modules.lock().unwrap().push("task".into());
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let cookie = f.login().await;
    let response = post(&f, &cookie, "contexts", json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
    let contexts: Value = response.json().await.unwrap();
    assert_eq!(contexts["contexts"].as_array().unwrap().len(), 1);
    assert_eq!(contexts["current"], contexts["contexts"][0]);
    assert_eq!(contexts["current"].as_object().unwrap().len(), 3);
    assert_eq!(
        contexts["projects"],
        json!([{"project_id":source.projects[0].1,"name":"project"}])
    );
    assert_eq!(
        contexts["current"]["project_id"],
        json!(source.projects[0].1)
    );
    let selected = json!({"project_dir":contexts["current"]["project_dir"],"project_version":contexts["current"]["project_version"]});
    let response = post(&f, &cookie, "schema", json!({"selection":selected})).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let schema: Value = response.json().await.unwrap();
    let response = post(&f, &cookie, "sample", json!({"selection":selected})).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let sample: Value = response.json().await.unwrap();
    assert_eq!(sample["generation"], schema["generation"]);
    let nodes = sample["nodes"].as_array().unwrap();
    let file = nodes.iter().find(|node| node["label"] == "File").unwrap();
    assert_eq!(
        file["primary_key"],
        json!({"name":"entity_key","value":{"type":"string","value":"main.rs"}})
    );
    let function = nodes
        .iter()
        .find(|node| node["label"] == "Function" && node["name"] == "α")
        .unwrap();
    assert_eq!(
        function["primary_key"],
        json!({"name":"entity_key","value":{"type":"string","value":"f1"}})
    );
    let response = post(&f, &cookie, "query", json!({"selection":selected,"query":{"query":"MATCH (f:File) RETURN f.name","parameters":{}}})).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let response = post(&f, &cookie, "neighborhood", json!({"selection":selected,"neighborhood":{
        "anchor_label":file["label"],"anchor_key":file["primary_key"]["value"],"relationship":"CONTAINS", "other_label":"Function","direction":"outgoing","limit":1}})).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["edges"][0]["uid"],
        "u0"
    );
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    let denied_origin = f
        .client
        .post(format!("{}/api/ast/schema", f.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, "http://untrusted.example")
        .json(&json!({"selection":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied_origin.status(), StatusCode::FORBIDDEN);
    let missing = f
        .client
        .post(format!("{}/api/ast/schema", f.origin))
        .header(header::ORIGIN, &f.origin)
        .json(&json!({"selection":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    let original = Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        graphit_core::file_lock::Cancellation::default(),
    ));
    let retained =
        crate::graph_server::tests::live_project_graph(&source, &graphs, &original).await;
    std::fs::rename(&destination, destination.with_extension("old")).unwrap();
    drop(graphit_fs::create_private_directory(&destination).unwrap());
    f.issuer.denied_modules.lock().unwrap().push("ast".into());
    let response = post(&f, &cookie, "contexts", json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"contexts":[],"current":null,"projects":[]})
    );
    assert_eq!(
        post(&f, &cookie, "schema", json!({"selection":{}}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    f.issuer
        .denied_modules
        .lock()
        .unwrap()
        .retain(|name| name != "ast");
    project_settings(&source, &[("modules.task", false), ("modules.ast", false)]);
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    let response = post(&f, &cookie, "contexts", json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"contexts":[],"current":null,"projects":[]})
    );
    assert_eq!(
        post(&f, &cookie, "schema", json!({"selection":{}}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    project_settings(&source, &[("modules.task", false), ("modules.ast", true)]);
    assert_eq!(
        post(&f, &cookie, "schema", json!({"selection":{}}))
            .await
            .status(),
        StatusCode::CONFLICT
    );
    drop(retained);
    f.finish_graphs().await;
}
#[tokio::test]
async fn cookie_ast_exact_input_and_output_limits_are_preserved() {
    let (mut source, graphs, _) = web_fixture().await;
    let f = open(&graphs, &mut source).await;
    let cookie = f.login().await;
    assert_eq!(
        post(
            &f,
            &cookie,
            "query",
            json!({"selection":{},"query":{"query":"RETURN 1"}})
        )
        .await
        .status(),
        StatusCode::OK
    );
    for (route, body) in [
        ("contexts", json!({"selection":{}})),
        ("schema", json!({"selection":{"project_version":"absent"}})),
        (
            "schema",
            json!({"selection":{"project_id":source.projects[0].1}}),
        ),
        (
            "query",
            json!({"selection":{},"query":{"query":"RETURN 1","parameters":[]}}),
        ),
    ] {
        assert_eq!(
            post(&f, &cookie, route, body).await.status(),
            StatusCode::BAD_REQUEST,
            "{route}"
        );
    }
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    let oversized = post(
        &f,
        &cookie,
        "query",
        json!({"selection":{},"query":{"query":"x".repeat(65536)}}),
    )
    .await;
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    let output = post(
        &f,
        &cookie,
        "query",
        json!({"selection":{},"query":{"query":"RETURN range(0, 20000) AS huge"}}),
    )
    .await;
    assert!(!output.status().is_success());
    let error = output.text().await.unwrap();
    assert!(error.len() <= 65536);
    assert!(
        error.contains("too large"),
        "expected the actual query byte-limit error: {error}"
    );
    f.finish_graphs().await;
}
#[tokio::test]
async fn cookie_ast_cancelled_admission_is_owned_and_next_request_remains_usable() {
    let (mut source, graphs, _) = web_fixture().await;
    let f = open(&graphs, &mut source).await;
    let cookie = f.login().await;
    f.issuer.hold_project.store(true, Ordering::SeqCst);
    let backend = Arc::new(task::backend(&format!(
        "{}/v1/project/access/authorize",
        f.issuer.origin
    )));
    let routes = crate::browser_auth::ast::Routes {
        sessions: f.service.clone(),
        graphs: graphs.clone(),
        sources: source.server.clone(),
        backend,
    };
    let mut jar = CookieJar::new();
    for value in cookie.split(';') {
        jar = jar.add(Cookie::parse(value.trim().to_owned()).unwrap().into_owned());
    }
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, f.origin.parse().unwrap());
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    let work = tokio::spawn(crate::browser_auth::ast::read(
        routes,
        jar,
        headers,
        axum::body::Bytes::from_static(b"{\"selection\":{}}"),
        crate::graph_server::BrowserOperation::Schema,
    ));
    tokio::time::timeout(Duration::from_secs(3), f.issuer.project_started.notified())
        .await
        .unwrap();
    let identity = tokio::time::timeout(Duration::from_secs(3), f.identity(&cookie))
        .await
        .expect("a pending AST admission must not hold the session credentials");
    assert_eq!(identity.status(), StatusCode::OK);
    assert!(!work.is_finished());
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    work.abort();
    assert!(work.await.unwrap_err().is_cancelled());
    f.issuer.hold_project.store(false, Ordering::SeqCst);
    f.issuer.release_project.notify_one();
    // Cancelling AST work keeps the same session usable. Session-expiry jobs
    // are joined only after shutdown cancels them.
    let next = tokio::time::timeout(
        Duration::from_secs(3),
        post(&f, &cookie, "schema", json!({"selection":{}})),
    )
    .await
    .unwrap();
    assert_eq!(next.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(3), f.finish_graphs())
        .await
        .unwrap();
}

#[tokio::test]
async fn cookie_ast_source_any_extension_without_graph_uses_real_selectors_and_read_ast() {
    let (mut source, graphs, destination) = web_fixture().await;
    std::fs::remove_dir_all(destination).unwrap();
    project_settings(&source, &[("modules.ast", false)]);
    std::fs::write(
        source.projects[0].0.root().join("arbitrary.unsupported"),
        "one\ntwo\nthree\nfour\nfive",
    )
    .unwrap();
    let f = open(&graphs, &mut source).await;
    f.issuer.denied_modules.lock().unwrap().push("task".into());
    let cookie = f.login().await;
    assert_eq!(
        post(&f, &cookie, "schema", json!({"selection":{}}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let body = |options: Value| {
        let mut source = json!({"path":"arbitrary.unsupported"});
        source
            .as_object_mut()
            .unwrap()
            .extend(options.as_object().unwrap().clone());
        json!({"selection":{},"source":source})
    };
    for (options, expected, start, end) in [
        (json!({"head":3,"tail":1}), "three", 3, 3),
        (
            json!({"start_line":2,"end_line":4}),
            "two\nthree\nfour",
            2,
            4,
        ),
        (
            json!({"pattern":"^THREE$","regex":true,"before":1,"after":1}),
            "",
            0,
            0,
        ),
        (
            json!({"pattern":"THREE","before":1,"after":1}),
            "     2: two\n>    3: three\n     4: four",
            0,
            0,
        ),
        (
            json!({"tail":2,"line_numbers":true}),
            "   4: four\n   5: five",
            4,
            5,
        ),
    ] {
        let response = post(&f, &cookie, "source", body(options)).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        let result: Value = response.json().await.unwrap();
        assert_eq!(result["origin"], "physical");
        assert_eq!(result["selection"]["source"], expected);
        assert_eq!(result["selection"]["total_lines"], 5);
        if start > 0 {
            assert_eq!(result["selection"]["start_line"], start);
            assert_eq!(result["selection"]["end_line"], end);
        }
    }
    for value in [
        body(json!({"head":-1})),
        body(json!({"head":null})),
        body(json!({"unknown":true})),
        json!({"selection":{},"source":{"path":"../outside"}}),
    ] {
        assert_eq!(
            post(&f, &cookie, "source", value).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    f.issuer.denied_modules.lock().unwrap().push("ast".into());
    assert_eq!(
        post(&f, &cookie, "source", body(json!({}))).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    f.finish_graphs().await;
}

#[tokio::test]
async fn cookie_ast_source_entity_has_real_raw_association_and_global_lines() {
    for stored in [true, false] {
        let (mut source, graphs) = crate::graph_server::tests::web_source_fixture(stored).await;
        let f = open(&graphs, &mut source).await;
        let cookie = f.login().await;
        let body = |name: &str, options: Value| {
            let mut source = json!({"path":"main.rs","entity":name,"entity_type":"Function"});
            source
                .as_object_mut()
                .unwrap()
                .extend(options.as_object().unwrap().clone());
            json!({"selection":{},"source":source})
        };
        let response = post(
            &f,
            &cookie,
            "source",
            body("hello", json!({"head":2,"tail":1})),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        let value: Value = response.json().await.unwrap();
        assert_eq!(value["origin"], if stored { "indexed" } else { "physical" });
        assert_eq!(value["selection"]["source"], "  work();");
        assert_eq!(value["selection"]["start_line"], 3);
        assert_eq!(value["selection"]["end_line"], 3);
        assert_eq!(value["selection"]["total_lines"], 5);
        for (name, status) in [
            ("duplicate", StatusCode::BAD_REQUEST),
            ("invalid", StatusCode::BAD_REQUEST),
            ("missing", StatusCode::NOT_FOUND),
            ("nonexact", StatusCode::NOT_FOUND),
        ] {
            assert_eq!(
                post(&f, &cookie, "source", body(name, json!({})))
                    .await
                    .status(),
                status
            );
        }
        std::fs::write(
            source.projects[0].0.root().join("main.rs"),
            "unrelated bytes",
        )
        .unwrap();
        let response = post(&f, &cookie, "source", body("hello", json!({}))).await;
        if stored {
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.json::<Value>().await.unwrap()["selection"]["source"],
                "fn hello() {\n  work();\n}"
            );
        } else {
            assert_eq!(response.status(), StatusCode::CONFLICT);
        }
        if stored {
            use arrow_array::{ArrayRef, RecordBatch, StringArray};
            let raw_path = source
                .auth
                .installation()
                .live
                .owner_root()
                .join("projects")
                .join(source.projects[0].1.to_string())
                .join("ast")
                .join("source.lance");
            let schema = graphit_ast::indexed_source::source_schema();
            let store = graphit_data::lance_module_store::LanceModuleStore::open(
                raw_path.to_str().unwrap(),
                Arc::clone(&schema),
            )
            .await
            .unwrap();
            let same = RecordBatch::try_new(
                schema,
                vec![
                    Arc::new(StringArray::from(vec!["main.rs"])) as ArrayRef,
                    Arc::new(StringArray::from(vec![Some(
                        "before\nfn hello() {\n  work();\n}\nafter",
                    )])) as ArrayRef,
                ],
            )
            .unwrap();
            let next = store
                .snapshot()
                .prepare_replace(vec![same])
                .await
                .unwrap()
                .commit()
                .await
                .unwrap();
            assert!(next.snapshot().version() > store.snapshot().version());
            let response = post(&f, &cookie, "source", body("hello", json!({}))).await;
            assert_eq!(response.status(), StatusCode::OK);
            let value: Value = response.json().await.unwrap();
            assert_eq!(value["origin"], "indexed");
            assert_eq!(value["selection"]["source"], "fn hello() {\n  work();\n}");
        }
        f.finish_graphs().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cookie_ast_imports_have_independent_authority_and_real_graph_source_readers() {
    let (mut source, graphs) = crate::graph_server::tests::web_import_fixture().await;
    let f = open(&graphs, &mut source).await;
    f.issuer.project_grant.store(false, Ordering::SeqCst);
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let cookie = f.login().await;
    let response = post(&f, &cookie, "imports", json!({})).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"imports":[{"id":"actual-graph","version":"web-import","context":"actual-graph@web-import","backend":"native"}]})
    );
    let selected = json!({"id":"actual-graph","version":"web-import"});
    let response = post(&f, &cookie, "imported/schema", json!({"source":selected})).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let schema: Value = response.json().await.unwrap();
    let response = post(&f, &cookie, "imported/sample", json!({"source":selected})).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let sample: Value = response.json().await.unwrap();
    assert_eq!(sample["generation"], schema["generation"]);
    let file = sample["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["label"] == "File")
        .unwrap();
    assert_eq!(
        file["primary_key"],
        json!({"name":"path","value":{"type":"string","value":"main.rs"}})
    );
    let response = post(&f, &cookie, "imported/query", json!({"source":selected,"query":{"query":"MATCH (f:Function) WHERE f.name = $name RETURN f.name","parameters":{"name":"hello"}}})).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    assert!(
        response
            .json::<Value>()
            .await
            .unwrap()
            .to_string()
            .contains("hello")
    );
    let response = post(&f, &cookie, "imported/neighborhood", json!({"source":selected,"neighborhood":{"anchor_label":"File","anchor_key":file["primary_key"]["value"],"relationship":"CONTAINS","other_label":"Function","direction":"outgoing","limit":1}})).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    assert_eq!(
        response.json::<Value>().await.unwrap()["edges"][0]["uid"],
        "u0"
    );
    let response = post(&f, &cookie, "imported/source", json!({"source":selected,"input":{"path":"main.rs","entity":"hello","entity_type":"Function","tail":1}})).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let inspected: Value = response.json().await.unwrap();
    assert_eq!(inspected["origin"], "indexed");
    assert_eq!(inspected["selection"]["source"], "last");
    assert_eq!(inspected["selection"]["start_line"], 4);
    let response = post(
        &f,
        &cookie,
        "imported/source",
        json!({"source":selected,"input":{"path":"main.rs","head":1}}),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    assert_eq!(
        response.json::<Value>().await.unwrap()["selection"]["source"],
        "first"
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(post(&f,&cookie,"imported/source",json!({"source":selected,"input":{"path":"main.rs","entity":"dup","entity_type":"Function"}})).await.status(),StatusCode::BAD_REQUEST);
    assert_eq!(
        post(
            &f,
            &cookie,
            "imported/schema",
            json!({"source":{"id":"actual-graph","version":"absent"}})
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post(
            &f,
            &cookie,
            "imports",
            json!({"project_dir":source.projects[0].0.root()})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let origin = f
        .client
        .post(format!("{}/api/ast/imports", f.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, "http://untrusted.example")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(origin.status(), StatusCode::FORBIDDEN);
    let response = f
        .client
        .post(format!("{}/api/ast/imports", f.origin))
        .header(header::ORIGIN, &f.origin)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    f.finish_graphs().await;
}

#[tokio::test]
async fn cookie_ast_imports_default_deny_despite_valid_web_and_project_permission() {
    let (mut source, graphs, _) = web_fixture().await;
    let f = open(&graphs, &mut source).await;
    let cookie = f.login().await;
    assert_eq!(
        post(&f, &cookie, "imports", json!({})).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
    f.finish_graphs().await;
}

#[tokio::test]
async fn cookie_ast_search_validates_web_project_admission_and_closed_semantic_input() {
    let (mut source, graphs, _) = web_fixture().await;
    let f = open(&graphs, &mut source).await;
    let cookie = f.login().await;
    for search in [
        json!({"query":"code","metric":"unknown"}),
        json!({"query":"code","k":0}),
        json!({"query":"code","project_version":"another"}),
    ] {
        assert_eq!(
            post(
                &f,
                &cookie,
                "search",
                json!({"selection":{},"search":search})
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        post(
            &f,
            &cookie,
            "search",
            json!({"selection":{},"search":{"query":"code","mode":"semantic"}})
        )
        .await
        .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        post(
            &f,
            &cookie,
            "search",
            json!({"selection":{},"search":{"query":"code","mode":"hybrid"}})
        )
        .await
        .status(),
        StatusCode::OK,
        "hybrid is a supported search mode"
    );
    let before_denial = f.issuer.project_calls.load(Ordering::SeqCst);
    f.issuer.denied_modules.lock().unwrap().push("ast".into());
    assert_eq!(
        post(
            &f,
            &cookie,
            "search",
            json!({"selection":{},"search":{"query":"code","mode":"vector"}})
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.issuer.project_calls.load(Ordering::SeqCst),
        before_denial + 1
    );
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    f.finish_graphs().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cookie_ast_search_uses_foreground_cpu_model_raw_nn_and_web_service_admission() {
    let (_storage, foreground, tasks, sources, raw, selected) =
        crate::foreground_source::foreground_memory_jobs_tests::browser_ast_search_fixture().await;
    let f = Fixture::with_graphs(policy(), (sources.clone(), raw)).await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    f.issuer.denied_modules.lock().unwrap().push("task".into());
    let cookie = f.login().await;
    let selection = json!({"project_dir":graphit_core::native_path::NativePath::new(selected.0.root().to_owned()).unwrap(),"project_version":selected.2.version()});
    for mode in ["semantic", "vector"] {
        let response = post(&f, &cookie, "search", json!({"selection":selection,"search":{"query":"browser_search_sum","mode":mode,"k":8,"metric":"cosine"}})).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        let result: Value = response.json().await.unwrap();
        assert_eq!(result["status"], "available");
        assert!(result.get("nn_generation").is_none());
        assert!(result.get("raw_generation").is_none());
        assert!(
            result["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(
                    |hit| hit["name"] == "browser_search_sum" && hit["path"] == "browser_search.rs"
                )
        );
    }
    let config = graphit_core::config::ConfigStore::project(&selected.0)
        .unwrap()
        .path();
    let mut value: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    value["settings"]["modules.embedding"] = json!(false);
    std::fs::write(config, serde_json::to_vec(&value).unwrap()).unwrap();
    let response = post(
        &f,
        &cookie,
        "search",
        json!({"selection":selection,"search":{"query":"browser_search_sum","mode":"fts","k":8}}),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let lexical: Value = response.json().await.unwrap();
    assert_eq!(lexical["status"], "available");
    assert_eq!(lexical["mode"], "fts");
    assert!(lexical.get("search_generation").is_none());
    let hit = lexical["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hit| hit["name"] == "browser_search_sum" && hit["kind"] == "Function")
        .unwrap();
    assert_eq!(hit["path"], "browser_search.rs");
    assert!(
        hit["entity_key"]
            .as_str()
            .is_some_and(|key| !key.is_empty())
    );
    assert!(hit["score"].as_f64().is_some_and(f64::is_finite));
    assert!(
        hit.get("content").is_none(),
        "search does not retain or deliver bodies"
    );
    let disabled: Value = post(
        &f,
        &cookie,
        "search",
        json!({"selection":selection,"search":{"query":"browser_search_sum","mode":"semantic"}}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(disabled["status"], "model_disabled");
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    f.issuer.denied_modules.lock().unwrap().push("ast".into());
    assert_eq!(
        post(
            &f,
            &cookie,
            "search",
            json!({"selection":selection,"search":{"query":"browser_search_sum","mode":"semantic"}})
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(
            &f,
            &cookie,
            "search",
            json!({"selection":selection,"search":{"query":"browser_search_sum","mode":"fts"}})
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    f.finish_graphs().await;
    tasks.shutdown().await.unwrap();
    sources.shutdown().await.unwrap();
    drop(sources);
    foreground.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cookie_imported_ast_search_and_source_use_exported_same_copy_without_author_project() {
    let (issuer, first) = Fixture::start_issuer().await;
    let (storage, mut foreground, tasks, sources, raw, selected) =
        crate::foreground_source::foreground_memory_jobs_tests::browser_ast_import_search_fixture(
            &issuer.origin,
        )
        .await;
    let installed = storage.publish_ast_search_package(&foreground, &raw, &selected);
    assert!(installed.join("graph.icebug/source.lance").is_dir());
    assert!(installed.join("graph.icebug/corpus.lance").is_dir());
    assert!(!installed.join("source.lance").exists());
    let f = Fixture::configured_with_issuer(
        policy(),
        None,
        None,
        Some((sources.clone(), raw)),
        issuer,
        first,
    )
    .await;
    let cookie = f.login().await;
    let response = post(
        &f,
        &cookie,
        "search",
        json!({"selection":{},"search":{"query":"browser_search_sum","mode":"hybrid","k":8}}),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let hybrid: Value = response.json().await.unwrap();
    assert_eq!(hybrid["mode"], "hybrid");
    assert!(
        hybrid["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["name"] == "browser_search_sum"
                && hit["lexical_score"].is_number()
                && hit["distance"].is_number())
    );
    f.issuer.project_grant.store(false, Ordering::SeqCst);
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let artifact = json!({"id":"fixture/ast-search","version":"packaged-v1"});
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    let response = post(&f, &cookie, "imports", json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
    let catalog: Value = response.json().await.unwrap();
    assert!(catalog.to_string().contains("fixture/ast-search"));
    for mode in ["fts", "semantic", "vector", "hybrid"] {
        let response = post(&f, &cookie, "imported/search", json!({"source":artifact,"search":{"query":"browser_search_sum","mode":mode,"k":8,"metric":"cosine"}})).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        let result: Value = response.json().await.unwrap();
        assert_eq!(result["status"], "available");
        assert_eq!(result["mode"], mode);
        assert!(result.get("search_generation").is_none());
        let hit = result["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|hit| hit["name"] == "browser_search_sum")
            .unwrap();
        assert_eq!(hit["path"], "browser_search.rs");
        assert!(
            hit["entity_key"]
                .as_str()
                .is_some_and(|key| !key.is_empty())
        );
        assert!(hit.get("content").is_none());
        if mode == "fts" {
            assert!(hit["score"].as_f64().is_some_and(f64::is_finite));
        }
        if mode == "hybrid" {
            assert!(hit["score"].as_f64().is_some_and(f64::is_finite));
        }
    }
    for selector in [
        json!({"head":1}),
        json!({"tail":1}),
        json!({"start_line":1,"end_line":1,"line_numbers":true}),
    ] {
        let mut source = json!({"path":"browser_search.rs"});
        source
            .as_object_mut()
            .unwrap()
            .extend(selector.as_object().unwrap().clone());
        let response = post(
            &f,
            &cookie,
            "imported/source",
            json!({"source":artifact,"input":source}),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        let result: Value = response.json().await.unwrap();
        assert_eq!(result["origin"], "indexed");
        assert_eq!(result["selection"]["total_lines"], 2);
        if selector.get("tail").is_some() {
            assert_eq!(result["selection"]["source"], "");
            assert_eq!(result["selection"]["start_line"], 2);
            assert_eq!(result["selection"]["end_line"], 2);
        } else {
            assert!(
                result["selection"]["source"]
                    .as_str()
                    .unwrap()
                    .contains("browser_search_sum")
            );
        }
    }
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(
        post(&f, &cookie, "schema", json!({"selection":{}}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let commands: Vec<_> = [None, Some("fixture/ast-search@packaged-v1")]
        .into_iter()
        .flat_map(|context| {
            ["fts", "semantic", "vector", "hybrid"].map(move |mode| {
                crate::graph_entry::Command::Search(crate::graph_entry::Search {
                    selection: crate::graph_entry::Selection {
                        context: context.map(str::to_owned),
                        ..Default::default()
                    },
                    query: "browser_search_sum".into(),
                    rerank: None,
                    mode: mode.into(),
                    k: Some(8),
                    metric: "cosine".into(),
                })
            })
        })
        .collect();
    for (index, result) in storage
        .run_ast_graph_commands(&mut foreground, &sources, &commands, &[])
        .await
        .into_iter()
        .enumerate()
    {
        let mode = ["fts", "semantic", "vector", "hybrid"][index % 4];
        assert_eq!(result["status"], "available");
        if index >= 4 || matches!(mode, "fts" | "hybrid") {
            assert_eq!(result["mode"], mode);
        }
        assert!(
            result["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|hit| hit["name"] == "browser_search_sum")
        );
    }
    f.finish_graphs().await;
    tasks.shutdown().await.unwrap();
    sources.shutdown().await.unwrap();
    drop(sources);
    foreground.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_ast_export_project_and_context_publish_confirmed_same_copy() {
    use futures_util::TryStreamExt;
    use graphit_core::artifact_store::CapturedSourceReader;
    use graphit_hub::{
        native_package::{self, PackageKind},
        policy::RequestContext,
    };
    let (issuer, _first) = Fixture::start_issuer().await;
    let (storage, mut foreground, tasks, sources, raw, selected) =
        crate::foreground_source::foreground_memory_jobs_tests::browser_ast_import_search_fixture(
            &issuer.origin,
        )
        .await;
    let installed = storage.publish_ast_search_package(&foreground, &raw, &selected);
    let outputs = tempfile::tempdir().unwrap();
    let exports = [
        crate::ast_export_entry::Arguments {
            output: outputs.path().join("project.zip"),
            project_dir: Some(selected.0.root().into()),
            context: None,
        },
        crate::ast_export_entry::Arguments {
            output: outputs.path().join("installed.zip"),
            project_dir: None,
            context: Some("fixture/ast-search@packaged-v1".into()),
        },
    ];
    let results = storage
        .run_ast_graph_commands(&mut foreground, &sources, &[], &exports)
        .await;
    let cancellation = graphit_core::file_lock::Cancellation::default();
    let original = RequestContext::new(
        sources.policy().content,
        &cancellation,
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    for (index, (result, args)) in results.iter().zip(&exports).enumerate() {
        assert_eq!(result["status"], "exported");
        assert_eq!(result["artifact_type"], "ast");
        assert!(result.get("graph_generation").is_none());
        let input = CapturedSourceReader::capture(
            &args.output,
            graphit_fs::NodeKind::File,
            &sources.policy().file,
            sources.policy().read,
            Arc::clone(original.snapshot()),
        )
        .unwrap();
        let unpacked = outputs.path().join(format!("unpacked-{index}"));
        native_package::import(
            &input,
            Some(PackageKind::Ast),
            &unpacked,
            sources.policy().native_install.hub.copy(),
            original.snapshot(),
        )
        .unwrap();
        for relative in ["graph.icebug/source.lance", "graph.icebug/corpus.lance"] {
            assert!(unpacked.join(relative).is_dir(), "{relative}");
            let expected = Box::pin(
                lance::dataset::builder::DatasetBuilder::from_uri(
                    installed.join(relative).to_str().unwrap(),
                )
                .load(),
            )
            .await
            .unwrap();
            let actual = Box::pin(
                lance::dataset::builder::DatasetBuilder::from_uri(
                    unpacked.join(relative).to_str().unwrap(),
                )
                .load(),
            )
            .await
            .unwrap();
            let expected = expected
                .scan()
                .try_into_stream()
                .await
                .unwrap()
                .try_collect::<Vec<_>>()
                .await
                .unwrap();
            let actual = actual
                .scan()
                .try_into_stream()
                .await
                .unwrap()
                .try_collect::<Vec<_>>()
                .await
                .unwrap();
            assert_eq!(actual, expected, "{relative}");
        }
        assert!(!unpacked.join("source.lance").exists());
        assert!(!unpacked.join("graph.icebug/source_bindings.json").exists());
    }
    tasks.shutdown().await.unwrap();
    drop(sources);
    foreground.shutdown().await.unwrap();
}
