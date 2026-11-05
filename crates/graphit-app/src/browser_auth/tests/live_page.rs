//! Real OAuth -> USER journal -> prepared SDK stamps -> historical HTTP evidence.
use super::*;
use crate::{
    knowledge_live::{HistoryLimits, LiveSessionService},
    task_server::{browser::records_fixture as native, tests::fixture_with_reference_permissions},
};
use graphit_auth::Permission;
use graphit_core::module_settings::Module;
use graphit_hub::NativeBinding;

async fn get(
    f: &Fixture,
    origin: &str,
    cookie: &str,
    id: &str,
    generation: &str,
    page: &str,
    context: Option<&Value>,
) -> reqwest::Response {
    let mut url =
        url::Url::parse(&format!("{origin}/api/live/sessions/{id}/knowledge/page")).unwrap();
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("generation", generation)
            .append_pair("page", page);
        if let Some(context) = context {
            query.append_pair("context", &context.to_string());
        }
    }
    f.client
        .get(url)
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap()
}
async fn json_ok(response: reqwest::Response) -> Value {
    let status = response.status();
    let url = response.url().clone();
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "actual evidence at {url}: {body}");
    serde_json::from_str(&body).unwrap()
}

#[tokio::test]
async fn live_evidence_reads_authorized_current_pages_after_close_and_head_updates() {
    let (temp, tasks, raw) = fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Hub),
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &["second", "empty"],
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
    let second = native::add_current_project(&tasks, &temp.path().join("second"));
    let empty = native::add_current_project(&tasks, &temp.path().join("empty"));
    live_execute::configure(&tasks);
    let roster = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("actual projects"));
    let first_id = roster.projects[0].1;
    let second_id = roster.projects[1].1;
    use graphit_data::knowledge_references::{
        KnowledgeDocumentState, KnowledgeReferenceFields, KnowledgeReferenceNamespace,
        KnowledgeReferenceScope,
    };
    let exported = native::publish(
        &tasks,
        &raw,
        0,
        &[native::page("same", "Imported title", &["literal target"])],
        Some(&[KnowledgeReferenceFields {
            slug: "same",
            state: KnowledgeDocumentState::Active,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(first_id),
                context: None,
            },
            authored: &[],
        }]),
    )
    .await;
    native::publish(
        &tasks,
        &raw,
        1,
        &[native::page("other-only", "Other selected project", &[])],
        Some(&[KnowledgeReferenceFields {
            slug: "other-only",
            state: KnowledgeDocumentState::Active,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(second_id),
                context: None,
            },
            authored: &[],
        }]),
    )
    .await;
    native::register_copy(&tasks, &exported, "vendor/live-evidence", "1.0.0", 0).await;
    native::update_preserving(
        &tasks,
        &raw,
        0,
        &[native::page("same", "Project title", &[])],
    )
    .await;
    let selections = live_execute::installed_texts(&tasks);
    let (_peer, _journal) = live_execute::attach_peer(temp.path(), &tasks);
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("actual project snapshot"));
    let resources = tasks.record_resources(&snapshot);
    let (_, id, version) = &resources.projects[0];
    let project = json!({"project_id":id.to_string(),"project_version":version.version()});
    let selected = json!({"source":"hub","project_id":id.to_string(),"instance_id":null,"project_kind":null,"type":"knowledge","id":"vendor/live-evidence","version":"1.0.0"});
    let default = json!({"source":"project","project_id":id.to_string(),"instance_id":null,"project_kind":null,"type":"knowledge","id":id.to_string(),"version":version.version()});
    let selected_project = json!({"source":"project","project_id":second["project_id"],"instance_id":null,
        "project_kind":null,"type":"knowledge","id":second["project_id"],"version":second["project_version"]});
    let mut f = Fixture::with_tasks(policy(), Some((tasks.clone(), raw.clone()))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
    let backend = Arc::new(task::backend(&format!(
        "{}/v1/project/access/authorize",
        f.issuer.origin
    )));
    let router = f
        .service
        .task_router(tasks.clone(), backend.clone(), None)
        .merge(f.service.live_execution_router(
            tasks.clone(),
            backend,
            address,
            graphit_executable(),
            None,
        ));
    let stop = f.stop.clone();
    f.servers.push(tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    }));
    let response = f
        .client
        .post(format!("{origin}/api/live/sessions"))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, &cookie)
        .json(&json!({"request_id":"http-evidence","project":project,
            "binding":NativeBinding::KiroV3,"agent":selections[0],"artifacts":[selected,selected_project],
            "model":"fixture-model"}))
        .send()
        .await
        .unwrap();
    let created = json_ok(response).await;
    let session = created["id"].as_str().unwrap();
    let generation = created["cursor"]["generation"].as_str().unwrap();
    assert_eq!(created["state"], "ready");
    let ambiguous =
        json_ok(get(&f, &origin, &cookie, session, generation, "same", None).await).await;
    let candidates = ambiguous["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    for candidate in candidates {
        assert!(candidate.get("content").is_none());
        assert!(candidate.get("body").is_none());
        assert_eq!(candidate["source_path"], "docs/records.md");
    }
    let page = json_ok(
        get(
            &f,
            &origin,
            &cookie,
            session,
            generation,
            "same",
            Some(&default),
        )
        .await,
    )
    .await;
    assert_eq!(page["title"], "Project title");
    assert_eq!(page["path"], "same");
    assert_eq!(page["content"], "# Complete native body é\n");
    assert_eq!(page["refs"]["kind"], "native");
    assert_eq!(page["refs"]["data"]["focused_source"]["slug"], "same");
    assert!(page["refs"]["data"].get("document").is_none());
    let imported = json_ok(
        get(
            &f,
            &origin,
            &cookie,
            session,
            generation,
            "same",
            Some(&selected),
        )
        .await,
    )
    .await;
    assert_eq!(imported["title"], "Imported title");
    assert_eq!(imported["context"], selected);
    assert_eq!(imported["refs"]["kind"], "imported");
    assert_eq!(
        imported["refs"]["data"]["focused_source"]["source_path"],
        "docs/records.md"
    );
    let namespace = imported["refs"]["data"]["focused_source"]["namespace"].clone();
    let witness = imported["refs"]["data"]["focused_source"]["witness"].clone();
    let reference = json!({"namespace":namespace,"witness":witness});
    let linked = json_ok(
        get_reference(
            &f, &origin, &cookie, session, generation, "same", &reference,
        )
        .await,
    )
    .await;
    assert_eq!(linked["context"], selected);
    assert_eq!(linked["title"], "Imported title");
    let ambiguous_ref = json!({"namespace":namespace,"witness":null});
    let ambiguous = json_ok(
        get_reference(
            &f,
            &origin,
            &cookie,
            session,
            generation,
            "same",
            &ambiguous_ref,
        )
        .await,
    )
    .await;
    assert_eq!(ambiguous["candidates"].as_array().unwrap().len(), 2);
    assert!(
        ambiguous["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|candidate| candidate.get("content").is_none())
    );
    let other_ref =
        json!({"namespace":{"scope":{"project":second_id},"context":null},"witness":null});
    let other = json_ok(
        get_reference(
            &f,
            &origin,
            &cookie,
            session,
            generation,
            "other-only",
            &other_ref,
        )
        .await,
    )
    .await;
    assert_eq!(other["context"], selected_project);
    assert_eq!(other["title"], "Other selected project");
    let wrong_ref = json!({"namespace":namespace,"witness":vec![0u8;32]});
    assert_eq!(
        get_reference(
            &f, &origin, &cookie, session, generation, "same", &wrong_ref
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    f.issuer
        .denied_projects
        .lock()
        .unwrap()
        .push(json!(second_id));
    assert_eq!(
        json_ok(
            get_reference(
                &f, &origin, &cookie, session, generation, "same", &reference
            )
            .await
        )
        .await["context"],
        selected
    );
    assert_eq!(
        get_reference(
            &f,
            &origin,
            &cookie,
            session,
            generation,
            "other-only",
            &other_ref
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    f.issuer.denied_projects.lock().unwrap().clear();
    assert_eq!(
        get(
            &f,
            &origin,
            &cookie,
            session,
            "stale-generation",
            "same",
            Some(&default)
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    let mut wrong = selected.clone();
    wrong["version"] = json!("9.9.9");
    assert_eq!(
        get(
            &f,
            &origin,
            &cookie,
            session,
            generation,
            "same",
            Some(&wrong)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(
            &f,
            &origin,
            &cookie,
            session,
            generation,
            "missing",
            Some(&default)
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let closed = f
        .client
        .post(format!("{origin}/api/live/sessions/{session}/close"))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(closed.status(), StatusCode::OK);
    assert_eq!(
        json_ok(
            get(
                &f,
                &origin,
                &cookie,
                session,
                generation,
                "same",
                Some(&selected)
            )
            .await
        )
        .await["title"],
        "Imported title"
    );
    f.issuer
        .denied_modules
        .lock()
        .unwrap()
        .push("knowledge".into());
    assert_eq!(
        get(
            &f,
            &origin,
            &cookie,
            session,
            generation,
            "same",
            Some(&default)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    f.issuer.denied_modules.lock().unwrap().clear();
    native::update_preserving(
        &tasks,
        &raw,
        0,
        &[native::page("same", "Changed title", &[])],
    )
    .await;
    let changed = get(
        &f,
        &origin,
        &cookie,
        session,
        generation,
        "same",
        Some(&default),
    )
    .await;
    assert_eq!(json_ok(changed).await["title"], "Changed title");
    assert_eq!(
        json_ok(
            get(
                &f,
                &origin,
                &cookie,
                session,
                generation,
                "same",
                Some(&selected)
            )
            .await
        )
        .await["title"],
        "Imported title"
    );
    // A previously absent derived corpus may become available to a fresh request.
    let response=f.client.post(format!("{origin}/api/live/sessions"))
        .header(header::ORIGIN,&f.origin).header(header::COOKIE,&cookie)
        .json(&json!({"request_id":"http-evidence-empty","project":{
            "project_id":empty["project_id"],"project_version":empty["project_version"]},
            "binding":NativeBinding::KiroV3,"agent":selections[0],"artifacts":[],"model":"fixture-model"}))
        .send().await.unwrap();
    let empty_session = json_ok(response).await;
    let empty_id = empty_session["id"].as_str().unwrap();
    let empty_generation = empty_session["cursor"]["generation"].as_str().unwrap();
    assert_eq!(
        get(
            &f,
            &origin,
            &cookie,
            empty_id,
            empty_generation,
            "newly-published",
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    native::publish(
        &tasks,
        &raw,
        2,
        &[native::page("newly-published", "Actual new Source", &[])],
        None,
    )
    .await;
    let changed = get(
        &f,
        &origin,
        &cookie,
        empty_id,
        empty_generation,
        "newly-published",
        None,
    )
    .await;
    assert_eq!(json_ok(changed).await["title"], "Actual new Source");
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
}

async fn get_reference(
    f: &Fixture,
    origin: &str,
    cookie: &str,
    id: &str,
    generation: &str,
    page: &str,
    reference: &Value,
) -> reqwest::Response {
    let mut url =
        url::Url::parse(&format!("{origin}/api/live/sessions/{id}/knowledge/page")).unwrap();
    url.query_pairs_mut()
        .append_pair("generation", generation)
        .append_pair("page", page)
        .append_pair("reference", &reference.to_string());
    f.client
        .get(url)
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap()
}
