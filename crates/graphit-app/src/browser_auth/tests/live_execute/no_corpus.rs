//! Fresh HTTP reads observe Knowledge published after agent preparation.
use super::*;
use crate::knowledge_live::{Access, ReadContexts};
use graphit_auth::ScopeCapture;

/// Observe the production Ready record through fresh Web and USER admissions.
async fn prepared_contexts(f: &Fixture, ready: &Value) -> ReadContexts {
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
    let tasks = &f.tasks.as_ref().unwrap().0;
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("actual project snapshot"));
    let resources = tasks.record_resources(&snapshot);
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        resources.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let bearer = zeroize::Zeroizing::new(entry.credentials.lock().await.access_token().to_owned());
    let flow = f
        .service
        .0
        .flow(entry.provider)
        .unwrap_or_else(|_| panic!("actual OAuth flow"));
    let (provider, verifier) = &f.service.0.providers[entry.provider];
    let caller = flow
        .verify_access(
            &entry.registered,
            &bearer,
            BrokerRequestContext::new(original.snapshot()),
        )
        .await
        .unwrap();
    let issuer = resources.auth.issuer();
    let scope = crate::foreground_user_memory::capture(
        issuer,
        resources.auth.installation(),
        &caller,
        verifier,
        &original,
        resources.policy,
    )
    .unwrap();
    let binding = scope.binding().unwrap();
    let read = issuer
        .admit_web_user_knowledge_read(
            &caller,
            verifier,
            &original,
            provider,
            ScopeCapture {
                live: &scope,
                record: &binding,
            },
        )
        .unwrap();
    let access = Access::user(&read, None).unwrap();
    tasks
        .live_sessions()
        .unwrap()
        .read_contexts(
            &access,
            ready["id"].as_str().unwrap(),
            ready["cursor"]["generation"].as_str().unwrap(),
        )
        .unwrap()
        .contexts
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn web_live_reads_publication_without_new_preparation() {
    Box::pin(no_corpus_publication()).await;
}

async fn no_corpus_publication() {
    let (temp, tasks, raw) = fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Hub),
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &[],
        Some(&[
            Permission::ReadUser,
            Permission::MutateUser,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ]),
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let limits = HistoryLimits::capture(
        graphit_core::native_document_file::NativeDocumentFileLimits::new(
            2 * 1024 * 1024,
            4096,
            128,
            8192,
        )
        .unwrap(),
        tasks.browser_policy().0,
    )
    .unwrap();
    let tasks = tasks
        .with_live_sessions(Arc::new(LiveSessionService::new(limits)))
        .unwrap();
    configure(&tasks);
    let selections = installed_texts(&tasks);
    let (available, journal) = attach_peer(temp.path(), &tasks);
    let mut f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let tasks = f.tasks.as_ref().unwrap().0.clone();
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("actual project snapshot"));
    let resources = tasks.record_resources(&snapshot);
    let (_, id, version) = &resources.projects[0];
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
    let backend = Arc::new(task::backend(&format!(
        "{}/v1/project/access/authorize",
        f.issuer.origin
    )));
    let router = f
        .service
        .task_router(tasks.clone(), Arc::clone(&backend), None)
        .merge(
            f.service.live_execution_router(
                tasks.clone(),
                backend,
                address,
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("graphit"),
                None,
            ),
        );
    let stop = f.stop.clone();
    f.servers.push(tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    }));
    let create = json!({
        "request_id":"http-no-corpus-ready",
        "project":{"project_id":id.to_string(),"project_version":version.version()},
        "binding":NativeBinding::KiroV3,
        "agent":selections[0],
        "artifacts":[selections[1],selections[2]],
        "title":"Prepared absence","model":"fixture-model"
    });
    let response = request(
        &f,
        &origin,
        &cookie,
        reqwest::Method::POST,
        "/api/live/sessions",
        Some(&create),
    )
    .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "NoCorpus Create: {body}");
    let ready: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(ready["state"], "ready");
    let selections_before = prepared_contexts(&f, &ready).await;
    assert!(
        log(&journal).is_empty(),
        "Ready cannot start the native peer"
    );
    let page_path = |session: &Value| {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair(
                "generation",
                session["cursor"]["generation"].as_str().unwrap(),
            )
            .append_pair("page", "same")
            .finish();
        format!(
            "/api/live/sessions/{}/knowledge/page?{query}",
            session["id"].as_str().unwrap()
        )
    };
    let old_page = page_path(&ready);
    let response = request(&f, &origin, &cookie, reqwest::Method::GET, &old_page, None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let missing: Value = response.json().await.unwrap();
    assert_eq!(missing["error"]["code"], "not_found");

    // A separate genuine SDK publisher becomes visible to the next HTTP operation.
    Box::pin(native::publish(
        &tasks,
        &f.tasks.as_ref().unwrap().1,
        0,
        &[native::page("same", "Live productive Knowledge", &[])],
        None,
    ))
    .await;
    let response = request(&f, &origin, &cookie, reqwest::Method::GET, &old_page, None).await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "newly published page: {body}");
    let changed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(changed["title"], "Live productive Knowledge");
    assert_eq!(changed["content"], "# Complete native body é\n");
    assert_eq!(prepared_contexts(&f, &ready).await, selections_before);
    assert!(
        log(&journal).is_empty(),
        "page reads cannot launch the peer"
    );

    let mut fresh_create = create.clone();
    fresh_create["request_id"] = json!("http-published-ready");
    let response = request(
        &f,
        &origin,
        &cookie,
        reqwest::Method::POST,
        "/api/live/sessions",
        Some(&fresh_create),
    )
    .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "fresh Published Create: {body}");
    let fresh: Value = serde_json::from_str(&body).unwrap();
    assert_ne!(fresh["id"], ready["id"]);
    assert_eq!(fresh["state"], "ready");
    assert_eq!(prepared_contexts(&f, &fresh).await, selections_before);
    let response = request(
        &f,
        &origin,
        &cookie,
        reqwest::Method::GET,
        &page_path(&fresh),
        None,
    )
    .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "fresh Published page: {body}");
    let page: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(page["title"], "Live productive Knowledge");
    assert_eq!(page["content"], "# Complete native body é\n");
    assert_eq!(page["context"]["type"], "knowledge");

    let fresh_id = fresh["id"].as_str().unwrap();
    let turn = "http-published-send";
    let response = request(
        &f,
        &origin,
        &cookie,
        reqwest::Method::POST,
        &format!("/api/live/sessions/{fresh_id}/messages"),
        Some(&json!({"turn_id":turn,"text":PROMPT})),
    )
    .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "fresh Send: {body}");
    wait_for(&f, &origin, &cookie, fresh_id, turn, false).await;
    assert_eq!(
        log(&journal)
            .lines()
            .filter(|line| *line == "live_mcp_four_tools_joined")
            .count(),
        1
    );
    assert_eq!(
        request(
            &f,
            &origin,
            &cookie,
            reqwest::Method::POST,
            &format!("/api/live/sessions/{fresh_id}/cancel"),
            None,
        )
        .await
        .status(),
        StatusCode::OK
    );
    let terminal = wait_for_terminal(&f, &origin, &cookie, fresh_id, turn).await;
    assert_eq!(terminal["outcome"], "cancelled");
    assert_eq!(terminal["joined"], true);
    assert_eq!(terminal["clean"], true);

    let response = request(&f, &origin, &cookie, reqwest::Method::GET, &old_page, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let unchanged: Value = response.json().await.unwrap();
    assert_eq!(unchanged["title"], "Live productive Knowledge");
    assert_eq!(prepared_contexts(&f, &ready).await, selections_before);
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    tasks.shutdown().await.unwrap();
    available.service().unwrap().shutdown().await.unwrap();
    f.service.shutdown().await.unwrap();
}
