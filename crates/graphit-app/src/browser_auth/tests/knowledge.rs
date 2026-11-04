//! Real signed browser PROJECT reads of native FULL16 Knowledge publications.
use super::*;
use crate::task_server::{
    browser::{knowledge_fixture, memory_fixture, records_fixture},
    tests::{knowledge_workspace_fixture, reopen_browser},
};
use graphit_data::knowledge_document_projection::{
    AuthoredPageFields, AuthoredParameters, CompilationDigest32, PhysicalSourceDigest32,
    RecipeFields,
};
use std::{num::NonZeroUsize, time::UNIX_EPOCH};

fn page<'a>(slug: &'a str, body: &'a str) -> AuthoredPageFields<'a> {
    AuthoredPageFields {
        slug: Some(slug),
        title: Some(""),
        body: Some(body),
        kind: Some("guide"),
        summary: Some("Summary"),
        description: Some(""),
        summary_truncated: true,
        source_path: "docs/guide.md",
        physical_digest: Some(PhysicalSourceDigest32::from_bytes([7; 32])),
        compilation_digest: CompilationDigest32::from_bytes([9; 32]),
        recipe: RecipeFields {
            id: graphit_data::knowledge_document_projection::AUTHORED_RECIPE_ID,
            parameters: Some(AuthoredParameters {
                fallback_title: "Fallback",
                kind_on_absent: "note",
                max_summary_bytes: NonZeroUsize::new(512).unwrap(),
            }),
        },
        updated_at: Some(UNIX_EPOCH + Duration::new(1, 123)),
        stale_since: Some(UNIX_EPOCH - Duration::new(1, 7)),
        stale_after: Some(Duration::new(3, 13)),
        resolved_wiki_targets: &["literal target", "literal target", "a-first"],
    }
}
fn body(selection: &Value, input: Value) -> Value {
    json!({"selection":selection,"input":input})
}
async fn read(f: &Fixture, cookie: &str, operation: &str, body: Value) -> reqwest::Response {
    f.client
        .post(format!("{}/api/knowledge/{operation}", f.origin))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, cookie)
        .json(&body)
        .send()
        .await
        .unwrap()
}
async fn value(f: &Fixture, cookie: &str, op: &str, selection: &Value, input: Value) -> Value {
    let r = read(f, cookie, op, body(selection, input)).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(r.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(r.headers()[header::CONTENT_TYPE], "application/json");
    r.json().await.unwrap()
}

#[tokio::test]
async fn knowledge_lexical_search_pages_preview_and_own_read_work_without_model_or_userinfo() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let mut first = page("a-search", "needle first body");
    first.resolved_wiki_targets = &[];
    let unicode = format!("needle {}", "中文 ".repeat(600));
    let mut second = page("b-search", &unicode);
    second.summary = Some("");
    second.resolved_wiki_targets = &[];
    let mut third = page("c-search", "needle last body");
    third.resolved_wiki_targets = &[];
    let published = knowledge_fixture::seed(&tasks, &raw, 0, &[first, second, third]).await;
    let selection = memory_fixture::selection(&tasks, 0);
    let mirror = memory_fixture::selection(&tasks, 2);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    f.issuer.denied_modules.lock().unwrap().push("task".into());
    let query = json!({"query":"needle","mode":"fts","top_k":0,"page_size":1});
    let first = value(&f, &cookie, "search", &selection, query.clone()).await;
    assert_eq!(first["selection"], selection);
    assert_eq!(first["source"], json!({"kind":"project_knowledge"}));
    assert_eq!(first["native"]["state"], "hits");
    assert_eq!(first["native"]["total"], 3);
    assert_eq!(
        first["native"]["generation"]["version"],
        published.default.version
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut current = first;
    loop {
        let hits = current["native"]["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0]["lexical_score"].as_f64().unwrap().is_finite());
        assert!(hits[0].get("preview").is_none());
        assert!(hits[0].get("body").is_none());
        assert!(hits[0]["focused_source"]["namespace"].is_null());
        assert!(hits[0]["focused_source"]["witness"].is_null());
        assert!(
            seen.insert(
                hits[0]["focused_source"]["slug"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            )
        );
        let cursor = current["native"]["next_cursor"].clone();
        if cursor.is_null() {
            break;
        }
        let mut changed = query.clone();
        changed["cursor"] = cursor.clone();
        changed["preview"] = json!(true);
        assert_eq!(
            read(&f, &cookie, "search", body(&selection, changed))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        let mut continued = query.clone();
        continued["cursor"] = cursor;
        current = value(&f, &cookie, "search", &selection, continued).await;
    }
    assert_eq!(seen.len(), 3);
    let preview = value(
        &f,
        &cookie,
        "search",
        &selection,
        json!({"query":"needle","mode":"fts","preview":true,"page_size":100}),
    )
    .await;
    let unicode_hit = preview["native"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hit| hit["focused_source"]["slug"] == "b-search")
        .unwrap();
    assert_eq!(unicode_hit["preview"]["source"], "body");
    assert_eq!(unicode_hit["preview"]["truncated"], true);
    let end = unicode_hit["preview"]["range"]["end"].as_u64().unwrap() as usize;
    assert!(unicode.is_char_boundary(end));
    assert_eq!(
        unicode_hit["preview"]["text"],
        format!("{}…", &unicode[..end])
    );
    assert_eq!(
        value(
            &f,
            &cookie,
            "search",
            &selection,
            json!({"query":"absentword","mode":"fts"})
        )
        .await["native"]["state"],
        "no_hits"
    );
    assert_eq!(
        value(&f, &cookie, "search", &mirror, query.clone()).await["native"]["state"],
        "no_corpus"
    );
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    f.issuer
        .denied_modules
        .lock()
        .unwrap()
        .push("knowledge".into());
    assert_eq!(
        read(&f, &cookie, "search", body(&selection, query))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn reopened_heads_full16_dates_and_exact_native_paths_are_preserved_without_userinfo() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let mut absent_fields = page("z-tail", "Second body");
    absent_fields.description = None;
    absent_fields.physical_digest = None;
    absent_fields.updated_at = None;
    absent_fields.stale_since = None;
    absent_fields.stale_after = None;
    absent_fields.resolved_wiki_targets = &[];
    let published = knowledge_fixture::seed(
        &tasks,
        &raw,
        0,
        &[
            absent_fields,
            page("a-first", "# Exact body\n[link](literal target)"),
        ],
    )
    .await;
    tasks.shutdown().await.unwrap();
    let tasks = reopen_browser(&tasks);
    let selection = memory_fixture::selection(&tasks, 0);
    let mirror = memory_fixture::selection(&tasks, 2);
    assert_eq!(selection["project_id"], mirror["project_id"]);
    assert_ne!(selection["project_dir"], mirror["project_dir"]);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let heads = value(&f, &cookie, "heads", &selection, json!({})).await;
    assert_eq!(heads["selection"], selection);
    assert_eq!(heads["source"], json!({"kind":"project_knowledge"}));
    assert_eq!(heads["focused_source"], Value::Null);
    assert_eq!(
        heads["native"]["generation"],
        json!({"corpus_uri":published.default.corpus_uri,"version":published.default.version,"physical_branch":null,"inherited_target":null})
    );
    assert_eq!(
        heads["native"]["heads"],
        json!([{ "slug":"a-first","title":"","updated_at":"1000000123"},{"slug":"z-tail","title":"","updated_at":null}])
    );
    let found = value(&f, &cookie, "page", &selection, json!({"slug":"a-first"})).await;
    assert_eq!(found["native"]["generation"], heads["native"]["generation"]);
    assert_eq!(found["source"], heads["source"]);
    assert_eq!(found["focused_source"]["slug"], "a-first");
    assert_eq!(found["focused_source"]["namespace"], Value::Null);
    assert_eq!(found["focused_source"]["witness"], Value::Null);
    assert_eq!(found["focused_source"]["source_path"], "docs/guide.md");
    assert_eq!(
        found["focused_source"]["compilation_digest"],
        json!(vec![9; 32])
    );
    assert_eq!(
        found["native"]["page"]["document"],
        json!({
            "slug":"a-first","title":"","body":"# Exact body\n[link](literal target)","kind":"guide","summary":"Summary","description":"","summary_truncated":true,
            "source_path":"docs/guide.md","physical_digest":vec![7;32],"compilation_digest":vec![9;32],"recipe":graphit_data::knowledge_document_projection::AUTHORED_RECIPE_ID,
            "recipe_parameters":{"fallback_title":"Fallback","kind_on_absent":"note","max_summary_bytes":512},
            "updated_at":"1000000123","stale_since":"-1000000007","stale_after":{"seconds":3,"nanoseconds":13},
            "resolved_wiki_targets":["literal target","literal target","a-first"]
        })
    );
    let tail = value(&f, &cookie, "page", &selection, json!({"slug":"z-tail"})).await;
    for field in [
        "description",
        "physical_digest",
        "updated_at",
        "stale_since",
        "stale_after",
    ] {
        assert!(tail["native"]["page"]["document"][field].is_null());
    }
    assert_eq!(
        value(&f, &cookie, "page", &selection, json!({"slug":"missing"})).await["native"]["page"]["state"],
        "page_absent"
    );
    for op in ["heads", "page"] {
        let input = if op == "page" {
            json!({"slug":"a-first"})
        } else {
            json!({})
        };
        assert_eq!(
            value(&f, &cookie, op, &mirror, input).await["native"]["state"],
            "no_corpus"
        );
    }
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn knowledge_page_reference_binding_uses_observed_zero_edge_namespace_for_real_refs() {
    use graphit_data::knowledge_references::{
        KnowledgeDocumentState, KnowledgeReferenceFields, KnowledgeReferenceNamespace,
        KnowledgeReferenceScope,
    };
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let selection = memory_fixture::selection(&tasks, 0);
    let project = serde_json::from_value(selection["project_id"].clone()).unwrap();
    let mut authored = page("literal page", "Body from the real page reader");
    authored.resolved_wiki_targets = &[];
    let published = records_fixture::publish(
        &tasks,
        &raw,
        0,
        &[authored],
        Some(&[KnowledgeReferenceFields {
            slug: "literal page",
            state: KnowledgeDocumentState::Active,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(project),
                context: None,
            },
            authored: &[],
        }]),
    )
    .await;
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let found = value(
        &f,
        &cookie,
        "page",
        &selection,
        json!({"slug":"literal page"}),
    )
    .await;
    assert_eq!(found["selection"], selection);
    assert_eq!(found["source"], json!({"kind":"project_knowledge"}));
    let focused = &found["focused_source"];
    assert_eq!(focused["slug"], "literal page");
    assert_eq!(
        focused["namespace"],
        json!({"scope":{"project":project},"context":null})
    );
    assert_eq!(focused["witness"].as_array().unwrap().len(), 32);
    assert_eq!(
        found["native"]["generation"]["version"],
        published.default.version
    );
    assert_eq!(
        found["native"]["page"]["document"]["body"],
        "Body from the real page reader"
    );
    let references = f
        .client
        .post(format!("{}/api/records/references", f.origin))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, &cookie)
        .json(&json!({"selection":selection,"input":{
            "focus":{"kind":"knowledge","scope":focused["namespace"]["scope"],"id":focused["slug"]},
            "pages":[{"source":found["source"],"direction":"outgoing"}]
        }}))
        .send()
        .await
        .unwrap();
    assert_eq!(references.status(), StatusCode::OK);
    let references: Value = references.json().await.unwrap();
    let native = &references["sources"][0]["native"]["published"];
    assert_eq!(native["generation"], found["native"]["generation"]);
    assert_eq!(native["read"]["focused_source"], *focused);
    assert_eq!(native["read"]["outgoing"]["results"], json!([]));
    assert_eq!(references["outgoing_complete"], true);
    f.finish_tasks().await;
}

#[tokio::test]
async fn published_empty_and_missing_corpus_are_distinct() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    knowledge_fixture::seed(&tasks, &raw, 0, &[]).await;
    let selection = memory_fixture::selection(&tasks, 0);
    let other = memory_fixture::selection(&tasks, 1);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let empty = value(&f, &cookie, "heads", &selection, json!({})).await;
    assert_eq!(empty["native"]["state"], "published");
    assert_eq!(empty["native"]["heads"], json!([]));
    let page = value(&f, &cookie, "page", &selection, json!({"slug":"anything"})).await;
    assert_eq!(page["native"]["page"]["state"], "published_empty");
    assert_eq!(page["native"]["generation"], empty["native"]["generation"]);
    assert_eq!(
        value(&f, &cookie, "page", &other, json!({"slug":"anything"})).await["native"]["state"],
        "no_corpus"
    );
}

#[tokio::test]
async fn admitted_versions_capture_distinct_physical_branches_after_reopen() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let tasks = memory_fixture::with_named_version(&tasks, "feature/knowledge");
    let publication = knowledge_fixture::seed(&tasks, &raw, 3, &[page("a-first", "Feature")]).await;
    let named = publication.named.as_ref().unwrap();
    assert!(named.physical_branch.is_some());
    tasks.shutdown().await.unwrap();
    let tasks = reopen_browser(&tasks);
    let main = memory_fixture::selection(&tasks, 0);
    let feature = memory_fixture::selection(&tasks, 3);
    assert_eq!(main["project_dir"], feature["project_dir"]);
    assert_eq!(main["project_id"], feature["project_id"]);
    assert_ne!(main["project_version"], feature["project_version"]);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let default = value(&f, &cookie, "page", &main, json!({"slug":"a-first"})).await;
    let selected = value(&f, &cookie, "page", &feature, json!({"slug":"a-first"})).await;
    assert_eq!(
        default["native"]["generation"]["physical_branch"],
        Value::Null
    );
    assert_eq!(
        selected["native"]["generation"]["physical_branch"],
        json!(named.physical_branch)
    );
    assert_eq!(selected["selection"], feature);
    assert_eq!(default["selection"], main);
    assert_eq!(selected["native"]["page"]["document"]["body"], "Feature");
    assert_eq!(
        selected["native"]["generation"]["version"],
        json!(named.version)
    );
}

#[tokio::test]
async fn strict_arguments_duplicate_fields_and_unavailable_selection_fail_before_project_io() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    for (op, request) in [
        ("heads", json!({"input":{}})),
        ("heads", body(&selection, json!({"unknown":true}))),
        ("page", body(&selection, json!({"slug":null}))),
        ("page", body(&selection, json!({"slug":""}))),
        ("page", body(&selection, json!({"slug":"../outside"}))),
        ("page", body(&selection, json!({"slug":"a/b"}))),
        ("page", body(&selection, json!({"slug":"a\\b"}))),
        ("page", body(&selection, json!({"slug":"a\u{0000}b"}))),
        ("page", body(&selection, json!({"slug":"x".repeat(4097)}))),
    ] {
        assert_eq!(
            read(&f, &cookie, op, request).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        read(
            &f,
            &cookie,
            "page",
            body(&selection, json!({"slug":"x".repeat(65536)}))
        )
        .await
        .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    for duplicate in [
        format!(
            "{{\"selection\":{},\"input\":{{\"slug\":\"a\",\"slug\":\"b\"}}}}",
            selection
        ),
        format!(
            "{{\"selection\":{},\"selection\":{},\"input\":{{}}}}",
            selection, selection
        ),
    ] {
        assert_eq!(
            f.client
                .post(format!("{}/api/knowledge/page", f.origin))
                .header(header::ORIGIN, &f.origin)
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/json")
                .body(duplicate)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    for field in ["project_version", "project_id", "project_dir"] {
        let mut unavailable = selection.clone();
        unavailable[field] = match field {
            "project_version" => json!("unavailable"),
            "project_id" => {
                memory_fixture::selection(&f.tasks.as_ref().unwrap().0, 1)[field].clone()
            }
            _ => memory_fixture::selection(&f.tasks.as_ref().unwrap().0, 1)[field].clone(),
        };
        assert_eq!(
            read(&f, &cookie, "heads", body(&unavailable, json!({})))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn knowledge_authorization_is_independent_of_task_memory_jwt_and_origin() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    for module in ["task", "memory"] {
        *f.issuer.project_module.lock().unwrap() = Some(module.into());
        assert_eq!(
            read(&f, &cookie, "heads", body(&selection, json!({})))
                .await
                .status(),
            StatusCode::BAD_GATEWAY
        );
        let response = if module == "task" {
            f.task_read(&cookie, "view", json!({})).await
        } else {
            f.client
                .post(format!("{}/api/memory/list", f.origin))
                .header(header::ORIGIN, &f.origin)
                .header(header::COOKIE, &cookie)
                .json(&body(&selection, json!({})))
                .send()
                .await
                .unwrap()
        };
        assert_eq!(response.status(), StatusCode::OK);
    }
    *f.issuer.project_module.lock().unwrap() = None;
    f.issuer.project_grant.store(false, Ordering::Release);
    assert_eq!(
        read(&f, &cookie, "heads", body(&selection, json!({})))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    f.issuer.project_grant.store(true, Ordering::Release);
    assert_eq!(
        read(
            &f,
            "graphit_session=invalid",
            "heads",
            body(&selection, json!({}))
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.client
            .post(format!("{}/api/knowledge/heads", f.origin))
            .header(header::ORIGIN, "https://foreign.example")
            .header(header::COOKIE, &cookie)
            .json(&body(&selection, json!({})))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.client
            .get(format!("{}/api/knowledge/heads", f.origin))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[tokio::test]
async fn output_limit_rejects_a_full_document_without_a_healthy_prefix() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    knowledge_fixture::seed(&tasks, &raw, 0, &[page("a-first", "body")]).await;
    let selection = memory_fixture::selection(&tasks, 0);
    let (mut content, _, _) = tasks.browser_policy();
    content.max_output_bytes = 16;
    let tasks = tasks.references_test_policy(content, Duration::from_secs(30));
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    for (op, input) in [("heads", json!({})), ("page", json!({"slug":"a-first"}))] {
        let r = read(&f, &cookie, op, body(&selection, input)).await;
        assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let v: Value = r.json().await.unwrap();
        assert_eq!(v["error"]["code"], "response_limit");
        assert!(v.get("native").is_none());
    }
}

#[tokio::test]
async fn corrupt_unselected_tail_cannot_return_found_or_page_absent() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    knowledge_fixture::seed(
        &tasks,
        &raw,
        0,
        &[page("a-first", "valid"), page("z-tail", "invalid tail")],
    )
    .await;
    knowledge_fixture::corrupt_unselected_source_path(&tasks).await;
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    assert_eq!(
        value(&f, &cookie, "heads", &selection, json!({})).await["native"]["heads"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    for slug in ["a-first", "missing"] {
        let r = read(&f, &cookie, "page", body(&selection, json!({"slug":slug}))).await;
        assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let v: Value = r.json().await.unwrap();
        assert_eq!(v["error"]["code"], "invalid_knowledge_corpus");
        assert!(v.get("native").is_none());
    }
}

#[tokio::test]
async fn short_native_manifest_is_causal_error_and_both_owners_shutdown_without_panic() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    knowledge_fixture::seed(&tasks, &raw, 0, &[page("a-first", "body")]).await;
    knowledge_fixture::corrupt_manifest(&tasks);
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    for (op, input) in [("heads", json!({})), ("page", json!({"slug":"a-first"}))] {
        let r = read(&f, &cookie, op, body(&selection, input)).await;
        assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let v: Value = r.json().await.unwrap();
        assert!(v.get("native").is_none());
        assert!(matches!(
            v["error"]["code"].as_str(),
            Some("store_failure" | "branch_failure" | "artifact_failure")
        ));
    }
    f.service.shutdown().await.unwrap();
    f.tasks.as_ref().unwrap().0.shutdown().await.unwrap();
}

#[tokio::test]
async fn deadline_shared_stop_and_profile_removal_prevent_late_delivery() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(2));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    f.issuer.hold_project.store(true, Ordering::Release);
    let r = read(&f, &cookie, "heads", body(&selection, json!({}))).await;
    assert!(!r.status().is_success());
    assert!(r.json::<Value>().await.unwrap().get("native").is_none());
    f.issuer.hold_project.store(false, Ordering::Release);
    f.issuer.release_project.notify_one();
    assert_eq!(
        read(&f, &cookie, "heads", body(&selection, json!({})))
            .await
            .status(),
        StatusCode::OK
    );
    let (tasks, raw) = f.tasks.as_ref().unwrap();
    crate::task_server::tests::browser_provider(tasks, raw, &f.issuer.origin, true);
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    assert_eq!(
        read(&f, &cookie, "heads", body(&selection, json!({})))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    tasks.shutdown().await.unwrap();
    let r = read(
        &f,
        &cookie,
        "page",
        body(&selection, json!({"slug":"anything"})),
    )
    .await;
    assert!(!r.status().is_success());
}

#[tokio::test]
async fn expired_access_without_refresh_cannot_read_knowledge() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    f.issuer.access_seconds.store(1, Ordering::Release);
    f.issuer.issue_refresh.store(false, Ordering::Release);
    let cookie = f.login().await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    assert_eq!(
        read(&f, &cookie, "heads", body(&selection, json!({})))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn disconnected_request_remains_owned_until_the_original_deadline_and_join() {
    use tokio::io::AsyncWriteExt;
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(2));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    f.issuer.hold_project.store(true, Ordering::Release);
    let url = reqwest::Url::parse(&f.origin).unwrap();
    let mut socket = tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()))
        .await
        .unwrap();
    let payload = body(&selection, json!({})).to_string();
    let wire = format!(
        "POST /api/knowledge/heads HTTP/1.1\r\nHost: {}:{}\r\nOrigin: {}\r\nCookie: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        url.host_str().unwrap(),
        url.port().unwrap(),
        f.origin,
        cookie,
        payload.len(),
        payload
    );
    socket.write_all(wire.as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), f.issuer.project_started.notified())
        .await
        .unwrap();
    drop(socket);
    {
        let mut jobs = f.service.0.jobs.lock().await;
        tokio::time::timeout(Duration::from_secs(5), jobs.join_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    f.issuer.hold_project.store(false, Ordering::Release);
    f.issuer.release_project.notify_one();
    assert_eq!(
        read(&f, &cookie, "heads", body(&selection, json!({})))
            .await
            .status(),
        StatusCode::OK
    );
    f.service.shutdown().await.unwrap();
    f.tasks.as_ref().unwrap().0.shutdown().await.unwrap();
}

#[tokio::test]
async fn swapping_the_selected_project_path_during_admission_never_delivers_data() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    knowledge_fixture::seed(&tasks, &raw, 0, &[page("a-first", "body")]).await;
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    f.issuer.hold_project.store(true, Ordering::Release);
    let request = read(
        &f,
        &cookie,
        "page",
        body(&selection, json!({"slug":"a-first"})),
    );
    tokio::pin!(request);
    tokio::select! {r=&mut request=>panic!("admission did not hold: {}",r.status()),_=f.issuer.project_started.notified()=>{}}
    knowledge_fixture::swap_project_root(&f.tasks.as_ref().unwrap().0);
    f.issuer.hold_project.store(false, Ordering::Release);
    f.issuer.release_project.notify_one();
    let response = request.await;
    assert!(!response.status().is_success());
    assert!(
        response
            .json::<Value>()
            .await
            .unwrap()
            .get("native")
            .is_none()
    );
}

async fn attach_ask_fixture(
    temp: &std::path::Path,
    tasks: &crate::task_server::TaskServer,
    script: &str,
) -> Arc<crate::knowledge_ask_service::AskAvailability> {
    use std::os::unix::fs::PermissionsExt;
    let program = temp.join("codex");
    std::fs::write(&program, format!("#!/usr/bin/python3\n{script}")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let available = Arc::new(
        crate::knowledge_ask_service::AskAvailability::capture(
            graphit_ai::cli_provider::CliCompletionProvider::Codex,
            std::ffi::OsStr::new("codex"),
            vec![("PATH".into(), temp.as_os_str().to_owned())],
            temp,
            graphit_core::native_document_file::NativeDocumentFileLimits::new(
                65536, 4096, 128, 65536,
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let cancellation = graphit_core::file_lock::Cancellation::default();
    let original = graphit_hub::policy::RequestContext::new(
        tasks.browser_policy().0,
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    available.service().unwrap().ready(&original).await.unwrap();
    tasks.attach_ask(Some(Arc::clone(&available))).unwrap();
    assert!(Arc::ptr_eq(&tasks.ask().unwrap().unwrap(), &available));
    available
}
fn final_ask(text: &str) -> Value {
    let frame = text
        .split("\n\n")
        .find(|frame| frame.contains("event: final"))
        .expect("native final event");
    let data = frame
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .expect("final JSON");
    serde_json::from_str(data).unwrap()
}
#[tokio::test]
async fn browser_ask_streams_native_events_and_selected_pages_without_userinfo() {
    let (temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let published =
        knowledge_fixture::seed(&tasks, &raw, 0, &[page("Paris", "Literal selected body")]).await;
    let script = format!(
        r#"import json,sys,pathlib
pathlib.Path({prompt:?}).write_text(sys.stdin.read())
pathlib.Path({args:?}).write_text(json.dumps(sys.argv[1:]))
print('{{"type":"turn.started"}}',flush=True)
print('{{"type":"item.completed","item":{{"id":"answer","type":"agent_message","text":"observed browser answer"}}}}',flush=True)
print('{{"type":"turn.completed","usage":{{"input_tokens":8,"cached_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0,"cache_write_input_tokens":0}}}}',flush=True)
"#,
        prompt = temp.path().join("ask-prompt.json").to_str().unwrap(),
        args = temp.path().join("ask-args.json").to_str().unwrap()
    );
    let available = attach_ask_fixture(temp.path(), &tasks, &script).await;
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks.clone(), raw))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let response = read(&f, &cookie, "ask", body(&selection, json!({"query":"Literal question","selected_slugs":["Paris"],"model":"literal model;$(native)"}))).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let text = response.text().await.unwrap();
    let progress = text
        .find("event: progress")
        .expect("streamed native progress");
    let terminal = text.find("event: final").expect("native terminal");
    assert!(progress < terminal);
    assert!(text.contains("turn.started"));
    let result = final_ask(&text);
    assert_eq!(result["status"], "answer");
    assert_eq!(result["generation"]["version"], published.default.version);
    assert_eq!(result["provided_slugs"], json!(["Paris"]));
    assert_eq!(
        result["receipt"]["completion"]["answer"],
        "observed browser answer"
    );
    assert_eq!(
        result["receipt"]["completion"]["requested_model"],
        "literal model;$(native)"
    );
    assert_eq!(result["receipt"]["process"]["cleanup"], "Complete");
    assert!(result.get("citations").is_none());
    let prompt: Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("ask-prompt.json")).unwrap())
            .unwrap();
    assert_eq!(prompt["question"], "Literal question");
    assert_eq!(prompt["pages"][0]["slug"], "Paris");
    assert_eq!(prompt["pages"][0]["body"], "Literal selected body");
    assert!(prompt["instruction"].as_str().unwrap().contains("pages"));
    let args: Vec<String> =
        serde_json::from_slice(&std::fs::read(temp.path().join("ask-args.json")).unwrap()).unwrap();
    assert!(args.iter().any(|arg| arg == "literal model;$(native)"));
    std::fs::remove_file(temp.path().join("ask-prompt.json")).unwrap();
    let empty = read(
        &f,
        &cookie,
        "ask",
        body(
            &selection,
            json!({"query":"q","selected_slugs":[],"model":"m"}),
        ),
    )
    .await;
    assert_eq!(empty.status(), StatusCode::OK);
    assert_eq!(
        final_ask(&empty.text().await.unwrap())["status"],
        "empty_selection"
    );
    assert!(!temp.path().join("ask-prompt.json").exists());
    tasks.shutdown().await.unwrap();
    available.service().unwrap().shutdown().await.unwrap();
    f.service.shutdown().await.unwrap();
}
#[tokio::test]
async fn browser_ask_rejects_missing_project_grant_and_invalid_input_before_provider_launch() {
    let (temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    knowledge_fixture::seed(&tasks, &raw, 0, &[page("Paris", "body")]).await;
    let marker = temp.path().join("ask-launched");
    let script = format!(
        "import pathlib\npathlib.Path({:?}).write_text('launched')\n",
        marker.to_str().unwrap()
    );
    let available = attach_ask_fixture(temp.path(), &tasks, &script).await;
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks.clone(), raw))).await;
    let cookie = f.login().await;
    let input = json!({"query":"q","selected_slugs":["Paris"],"model":"m"});
    f.issuer.project_grant.store(false, Ordering::Release);
    let denied = read(&f, &cookie, "ask", body(&selection, input.clone())).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert!(
        !denied.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .contains("event-stream")
    );
    f.issuer.project_grant.store(true, Ordering::Release);
    for input in [
        json!({"query":"q","selected_slugs":["Paris"],"model":"m","actor":"spoofed"}),
        json!({"query":"q","selected_slugs":[""],"model":"m"}),
        json!({"query":"q","selected_slugs":[],"retrieval":{"mode":"fts","top_k":1},"model":"m"}),
        json!({"query":"q","model":"m"}),
        json!({"query":"q","retrieval":null,"model":"m"}),
        json!({"query":"q","retrieval":{"mode":"fts","top_k":0},"model":"m"}),
        json!({"query":"q","retrieval":{"mode":"fts","top_k":1,"metric":"dot"},"model":"m"}),
    ] {
        assert_eq!(
            read(&f, &cookie, "ask", body(&selection, input))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        read(&f, "invalid_cookie", "ask", body(&selection, input))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert!(!marker.exists());
    tasks.shutdown().await.unwrap();
    available.service().unwrap().shutdown().await.unwrap();
    f.service.shutdown().await.unwrap();
}
#[tokio::test]
async fn browser_ask_disconnect_cancels_same_request_and_task_shutdown_joins_child() {
    let (temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    knowledge_fixture::seed(&tasks, &raw, 0, &[page("Paris", "body")]).await;
    let pid = temp.path().join("ask-pid");
    let script = format!(
        r#"import os,pathlib,sys,time
sys.stdin.read()
pathlib.Path({:?}).write_text(str(os.getpid()))
print('{{"type":"turn.started"}}',flush=True)
time.sleep(60)
"#,
        pid.to_str().unwrap()
    );
    let available = attach_ask_fixture(temp.path(), &tasks, &script).await;
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks.clone(), raw))).await;
    let cookie = f.login().await;
    let mut response = read(
        &f,
        &cookie,
        "ask",
        body(
            &selection,
            json!({"query":"q","selected_slugs":["Paris"],"model":"m"}),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let chunk = response
        .chunk()
        .await
        .unwrap()
        .expect("real progress while child remains active");
    assert!(
        std::str::from_utf8(&chunk)
            .unwrap()
            .contains("turn.started")
    );
    let child: u32 = std::fs::read_to_string(&pid).unwrap().parse().unwrap();
    assert!(std::path::Path::new(&format!("/proc/{child}")).exists());
    drop(response);
    tokio::time::timeout(Duration::from_secs(20), tasks.shutdown())
        .await
        .unwrap()
        .unwrap();
    available.service().unwrap().shutdown().await.unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{child}")).exists());
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn imported_knowledge_cpu_web_preserves_corpus_cursor_and_independent_authority() {
    let (issuer, issuer_service) = Fixture::start_issuer().await;
    let (_storage, mut foreground, tasks, sources, raw, pair) =
        crate::foreground_source::foreground_memory_jobs_tests::imported_knowledge_cpu_search_fixture(Some(&issuer.origin)).await;
    // The shared Browser fixture installs the provider through the real Native manager.
    // Remove the startup record first so that installation remains a genuine add.
    crate::task_server::tests::browser_provider(&tasks, &raw, &issuer.origin, true);
    let f = Fixture::configured_with_issuer(
        policy(),
        Some((tasks.clone(), raw.clone())),
        None,
        None,
        issuer,
        issuer_service,
    )
    .await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    f.issuer
        .project_grant
        .store(false, std::sync::atomic::Ordering::Release);
    let source = json!({"kind":"imported_knowledge","artifact":{"id":"vendor/mount-a","version":"exact-release"}});
    let mut generations = None;
    let mut saved_cursor = None;
    for mode in ["semantic", "hybrid"] {
        let mut search = json!({"query":"Knowledge CPU search", "mode":mode,"top_k":2,"page_size":1,"preview":true});
        let mut slugs = std::collections::BTreeSet::new();
        for page in 0..2 {
            let response = read(
                &f,
                &cookie,
                "imported/search",
                json!({"input":{"source":source,"search":search}}),
            )
            .await;
            if response.status() != StatusCode::OK {
                let status = response.status();
                panic!(
                    "imported CPU {mode} page {page}: HTTP {status}: {}",
                    response.text().await.unwrap()
                );
            }
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            let value: Value = response.json().await.unwrap();
            assert_eq!(value["source"], source);
            assert!(value.get("selection").is_none_or(Value::is_null));
            let native = &value["native"];
            assert_eq!(native["state"], "hits");
            assert_eq!(native["total"], 2);
            assert_eq!(native["generation"]["version"], pair);
            assert_eq!(native["semantic_generation"]["version"], pair);
            let current = (
                native["generation"].clone(),
                native["semantic_generation"].clone(),
            );
            if let Some(expected) = &generations {
                assert_eq!(&current, expected);
            } else {
                generations = Some(current);
            }
            let corpus = std::path::Path::new(native["generation"]["corpus_uri"].as_str().unwrap());
            assert_eq!(corpus.file_name().unwrap(), "index.lance");
            assert_eq!(
                corpus.parent().unwrap().file_name().unwrap(),
                "i1-mounted-a"
            );
            let hits = native["hits"].as_array().unwrap();
            assert_eq!(hits.len(), 1);
            let hit = &hits[0];
            let slug = hit["focused_source"]["slug"].as_str().unwrap();
            let title = match slug {
                "B" => "Knowledge CPU search B first",
                "B-second" => "Knowledge CPU search B second",
                _ => panic!("unexpected imported slug {slug}"),
            };
            assert!(slugs.insert(slug.to_owned()));
            assert_eq!(hit["focused_source"]["title"], title);
            assert_eq!(hit["preview"]["text"], title);
            assert_eq!(hit["preview"]["source"], "summary");
            assert!(hit["focused_source"]["namespace"].is_null());
            assert!(hit["focused_source"]["witness"].is_null());
            assert!(hit["semantic_distance"].as_f64().unwrap().is_finite());
            assert!(hit.get("body").is_none());
            if mode == "hybrid" {
                assert!(hit["lexical_score"].as_f64().unwrap().is_finite());
                assert!(hit["rrf_score"].as_f64().unwrap().is_finite());
            } else {
                assert!(hit.get("lexical_score").is_none());
                assert!(hit.get("rrf_score").is_none());
            }
            if page == 0 {
                assert!(native["next_cursor"].is_object());
                search["cursor"] = native["next_cursor"].clone();
                if mode == "semantic" {
                    saved_cursor = Some(search["cursor"].clone());
                }
            } else {
                assert!(native["next_cursor"].is_null());
            }
        }
        assert_eq!(
            slugs,
            std::collections::BTreeSet::from(["B".to_owned(), "B-second".to_owned()])
        );
    }
    let response = read(&f, &cookie, "imported/search", json!({"input":{"source":source,"search":{"query":"changed query","mode":"semantic","top_k":2,"page_size":1,"cursor":saved_cursor.unwrap()}}})).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        response
            .json::<Value>()
            .await
            .unwrap()
            .get("native")
            .is_none()
    );
    let response = read(&f, "invalid-cookie", "imported/search", json!({"input":{"source":source,"search":{"query":"Knowledge CPU search","mode":"semantic","top_k":2,"page_size":1}}})).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    crate::foreground_source::foreground_memory_jobs_tests::stop_browser_model(&mut foreground)
        .await;
    tasks.attach_memory_model(None).unwrap();
    let (generation, semantic_generation) = generations.unwrap();
    for (mode, state) in [("semantic", "unavailable"), ("hybrid", "partial")] {
        let response = read(&f, &cookie, "imported/search", json!({"input":{"source":source,"search":{"query":"Knowledge CPU search","mode":mode,"top_k":2,"page_size":1}}})).await;
        assert_eq!(response.status(), StatusCode::OK);
        let value: Value = response.json().await.unwrap();
        assert_eq!(value["source"], source);
        let native = &value["native"];
        assert_eq!(native["state"], state);
        assert_eq!(native["reason"], "model_unavailable");
        assert_eq!(native["generation"], generation);
        assert_eq!(native["semantic_generation"], semantic_generation);
        let hits = native["hits"].as_array().unwrap();
        if mode == "semantic" {
            assert!(hits.is_empty());
        } else {
            assert_eq!(hits.len(), 1);
            assert!(hits[0]["lexical_score"].as_f64().unwrap().is_finite());
            assert!(hits[0].get("semantic_distance").is_none());
            assert!(hits[0].get("rrf_score").is_none());
        }
    }
    let missing = json!({"kind":"imported_knowledge","artifact":{"id":"vendor/mount-a","version":"missing-release"}});
    let response = read(&f, &cookie, "imported/search", json!({"input":{"source":missing,"search":{"query":"Knowledge CPU search","mode":"semantic","top_k":2,"page_size":1}}})).await;
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value = response.json().await.unwrap();
    assert_eq!(value["source"], missing);
    assert_eq!(value["native"]["state"], "registry_absent");
    assert_eq!(
        f.issuer
            .project_calls
            .load(std::sync::atomic::Ordering::Acquire),
        0
    );
    assert_eq!(
        f.issuer.userinfo.load(std::sync::atomic::Ordering::Acquire),
        0
    );
    crate::task_server::tests::browser_provider(&tasks, &raw, &f.issuer.origin, true);
    let response = read(&f, &cookie, "imported/search", json!({"input":{"source":source,"search":{"query":"Knowledge CPU search","mode":"semantic","top_k":2,"page_size":1}}})).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        response
            .json::<Value>()
            .await
            .unwrap()
            .get("native")
            .is_none()
    );
    f.service.shutdown().await.unwrap();
    tasks.shutdown().await.unwrap();
    sources.shutdown().await.unwrap();
    foreground.shutdown().await.unwrap();
}

#[tokio::test]
async fn knowledge_vector_search_uses_shared_cpu_and_preserves_same_pin_lexical_without_model() {
    let (_storage, mut foreground, tasks, sources, raw, selected) =
        crate::foreground_source::foreground_memory_jobs_tests::browser_knowledge_search_fixture()
            .await;
    let selection = memory_fixture::selection(&tasks, 0);
    let mut other = page("other-project", "Knowledge CPU search in another project");
    other.resolved_wiki_targets = &[];
    knowledge_fixture::seed(&tasks, &raw, 1, &[other]).await;
    let unindexed = memory_fixture::selection(&tasks, 1);
    let f = Fixture::with_tasks(policy(), Some((tasks.clone(), raw))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    f.issuer.denied_modules.lock().unwrap().push("task".into());
    let query =
        |mode| json!({"query":"Knowledge CPU search","mode":mode,"preview":true,"page_size":1});
    let semantic = value(&f, &cookie, "search", &selection, query("semantic")).await;
    assert_eq!(semantic["native"]["state"], "hits");
    assert!(
        semantic["native"]["semantic_generation"]["version"]
            .as_u64()
            .is_some()
    );
    let sh = &semantic["native"]["hits"][0];
    assert_eq!(sh["focused_source"]["slug"], "knowledge-cpu");
    assert!(sh["semantic_distance"].as_f64().is_some_and(f64::is_finite));
    assert!(sh.get("lexical_score").is_none());
    let hybrid = value(&f, &cookie, "search", &selection, query("hybrid")).await;
    assert_eq!(hybrid["native"]["state"], "hits");
    assert_eq!(
        hybrid["native"]["generation"],
        semantic["native"]["generation"]
    );
    let hh = &hybrid["native"]["hits"][0];
    assert!(hh["lexical_score"].as_f64().is_some_and(f64::is_finite));
    assert!(hh["semantic_distance"].as_f64().is_some_and(f64::is_finite));
    assert!(hh["rrf_score"].as_f64().is_some_and(f64::is_finite));
    assert_eq!(hh["preview"]["source"], "summary");
    assert!(hh.get("document").is_none());
    crate::foreground_source::foreground_memory_jobs_tests::stop_browser_model(&mut foreground)
        .await;
    // Missing NN readiness succeeds with an actually closed model endpoint: no Query RPC.
    let missing = value(&f, &cookie, "search", &unindexed, query("semantic")).await;
    assert_eq!(missing["native"]["state"], "unavailable");
    assert_eq!(missing["native"]["reason"], "missing_semantic");
    assert!(missing["native"]["hits"].as_array().unwrap().is_empty());
    let partial = value(&f, &cookie, "search", &unindexed, query("hybrid")).await;
    assert_eq!(partial["native"]["state"], "partial");
    assert_eq!(
        partial["native"]["hits"][0]["focused_source"]["slug"],
        "other-project"
    );
    assert!(partial["native"]["hits"][0].get("rrf_score").is_none());
    // Ready corpus must report a failed real Query transport, not degrade to lexical.
    assert_eq!(
        read(&f, &cookie, "search", body(&selection, query("semantic")))
            .await
            .status(),
        StatusCode::BAD_GATEWAY
    );
    graphit_core::config::ConfigStore::project(&selected.0)
        .unwrap()
        .set(
            graphit_core::config::ConfigKey::new("modules.embedding").unwrap(),
            json!(false),
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
    let disabled = value(&f, &cookie, "search", &selection, query("hybrid")).await;
    assert_eq!(disabled["native"]["state"], "partial");
    assert_eq!(
        disabled["native"]["hits"][0]["focused_source"]["slug"],
        "knowledge-cpu"
    );
    assert!(
        disabled["native"]["hits"][0]
            .get("semantic_distance")
            .is_none()
    );
    graphit_core::config::ConfigStore::project(&selected.0)
        .unwrap()
        .set(
            graphit_core::config::ConfigKey::new("modules.embedding").unwrap(),
            json!(true),
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
    tasks.attach_memory_model(None).unwrap();
    let no_model = value(&f, &cookie, "search", &selection, query("hybrid")).await;
    assert_eq!(no_model["native"]["state"], "partial");
    assert_eq!(no_model["native"]["reason"], "model_unavailable");
    assert_eq!(
        no_model["native"]["generation"],
        semantic["native"]["generation"]
    );
    assert_eq!(
        no_model["native"]["semantic_generation"],
        semantic["native"]["semantic_generation"]
    );
    assert_eq!(
        no_model["native"]["hits"][0]["focused_source"]["slug"],
        "knowledge-cpu"
    );
    assert!(
        no_model["native"]["hits"][0]
            .get("semantic_distance")
            .is_none()
    );
    assert!(no_model["native"]["hits"][0].get("rrf_score").is_none());
    let no_semantic = value(&f, &cookie, "search", &selection, query("semantic")).await;
    assert_eq!(no_semantic["native"]["state"], "unavailable");
    assert!(no_semantic["native"]["hits"].as_array().unwrap().is_empty());
    f.issuer
        .denied_modules
        .lock()
        .unwrap()
        .push("knowledge".into());
    assert_eq!(
        read(&f, &cookie, "search", body(&selection, query("hybrid")))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    f.service.shutdown().await.unwrap();
    tasks.shutdown().await.unwrap();
    sources.shutdown().await.unwrap();
    foreground.shutdown().await.unwrap();
}

#[tokio::test]
async fn browser_ask_retrieves_only_same_pin_pages_and_skips_provider_without_hits_or_model() {
    let (temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let published = knowledge_fixture::seed(
        &tasks,
        &raw,
        0,
        &[
            page("Paris", "travelneedle real selected body"),
            page("Lyon", "unselected confidential body"),
        ],
    )
    .await;
    let prompt_file = temp.path().join("auto-prompt.json");
    let script = format!(
        r#"import json,sys,pathlib
pathlib.Path({:?}).write_text(sys.stdin.read())
print('{{"type":"turn.started"}}',flush=True)
print('{{"type":"item.completed","item":{{"id":"answer","type":"agent_message","text":"retrieved answer"}}}}',flush=True)
print('{{"type":"turn.completed","usage":{{"input_tokens":8,"cached_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0,"cache_write_input_tokens":0}}}}',flush=True)
"#,
        prompt_file.to_str().unwrap()
    );
    let available = attach_ask_fixture(temp.path(), &tasks, &script).await;
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks.clone(), raw))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    f.issuer.denied_modules.lock().unwrap().push("task".into());
    for (mode, state) in [("fts", "hits"), ("hybrid", "partial")] {
        let response = read(
            &f,
            &cookie,
            "ask",
            body(
                &selection,
                json!({"query":"travelneedle","model":"m","retrieval":{"mode":mode,"top_k":1}}),
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let stream = response.text().await.unwrap();
        assert!(stream.find("event: progress").unwrap() < stream.find("event: final").unwrap());
        let result = final_ask(&stream);
        assert_eq!(result["status"], "answer");
        assert_eq!(result["provided_slugs"], json!(["Paris"]));
        assert_eq!(result["generation"]["version"], published.default.version);
        assert_eq!(
            result["retrieval"]["generation"]["version"],
            published.default.version
        );
        assert_eq!(result["retrieval"]["state"], state);
        assert_eq!(result["retrieval"]["selected_slugs"], json!(["Paris"]));
        assert!(result.get("citations").is_none());
        assert!(result["retrieval"].get("hits").is_none());
        let prompt: Value = serde_json::from_slice(&std::fs::read(&prompt_file).unwrap()).unwrap();
        assert_eq!(prompt["pages"].as_array().unwrap().len(), 1);
        assert_eq!(prompt["pages"][0]["slug"], "Paris");
        assert_eq!(
            prompt["pages"][0]["body"],
            "travelneedle real selected body"
        );
        std::fs::remove_file(&prompt_file).unwrap();
    }
    for (mode, query, expected) in [
        ("fts", "zzzzabsentterm", "no_hits"),
        ("semantic", "travelneedle", "unavailable"),
    ] {
        let response = read(
            &f,
            &cookie,
            "ask",
            body(
                &selection,
                json!({"query":query,"model":"m","retrieval":{"mode":mode,"top_k":1}}),
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            final_ask(&response.text().await.unwrap())["status"],
            expected
        );
        assert!(!prompt_file.exists());
    }
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    tasks.shutdown().await.unwrap();
    available.service().unwrap().shutdown().await.unwrap();
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn browser_ask_retrieval_uses_existing_cpu_web_lease_and_real_selected_documents() {
    let (_storage, foreground, tasks, sources, raw, _selected) =
        crate::foreground_source::foreground_memory_jobs_tests::browser_knowledge_search_fixture()
            .await;
    let temp = tempfile::tempdir().unwrap();
    let prompt_file = temp.path().join("auto-cpu-prompt.json");
    let script = format!(
        r#"import sys,pathlib
pathlib.Path({:?}).write_text(sys.stdin.read())
print('{{"type":"turn.started"}}',flush=True)
print('{{"type":"item.completed","item":{{"id":"answer","type":"agent_message","text":"CPU retrieved answer"}}}}',flush=True)
print('{{"type":"turn.completed","usage":{{"input_tokens":8,"cached_input_tokens":0,"output_tokens":3,"reasoning_output_tokens":0,"cache_write_input_tokens":0}}}}',flush=True)
"#,
        prompt_file.to_str().unwrap()
    );
    let available = attach_ask_fixture(temp.path(), &tasks, &script).await;
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks.clone(), raw))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    f.issuer.denied_modules.lock().unwrap().push("task".into());
    for mode in ["semantic", "hybrid"] {
        let response = read(&f, &cookie, "ask", body(&selection, json!({"query":"Knowledge CPU search","model":"m","retrieval":{"mode":mode,"top_k":1}}))).await;
        assert_eq!(response.status(), StatusCode::OK);
        let result = final_ask(&response.text().await.unwrap());
        assert_eq!(result["status"], "answer");
        assert_eq!(result["provided_slugs"], json!(["knowledge-cpu"]));
        assert_eq!(result["retrieval"]["state"], "hits");
        assert!(
            result["retrieval"]["semantic_generation"]["version"]
                .as_u64()
                .is_some()
        );
        assert_eq!(result["generation"], result["retrieval"]["generation"]);
        let prompt: Value = serde_json::from_slice(&std::fs::read(&prompt_file).unwrap()).unwrap();
        assert_eq!(prompt["pages"].as_array().unwrap().len(), 1);
        assert_eq!(prompt["pages"][0]["slug"], "knowledge-cpu");
        assert!(
            prompt["pages"][0]["body"]
                .as_str()
                .unwrap()
                .contains("Knowledge CPU")
        );
    }
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    f.service.shutdown().await.unwrap();
    tasks.shutdown().await.unwrap();
    sources.shutdown().await.unwrap();
    available.service().unwrap().shutdown().await.unwrap();
    foreground.shutdown().await.unwrap();
}
