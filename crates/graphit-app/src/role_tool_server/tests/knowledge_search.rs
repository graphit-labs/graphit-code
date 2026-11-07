use super::*;
use graphit_data::knowledge_document_projection as h;

async fn publish_first(server: &RoleToolServer, raw: &str, rows: &[(&str, &str, &str, &str)]) {
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let selected = project_snapshot(server);
    let (paths, id, version) = &selected.projects[0];
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.task.scope, original.snapshot())
            .unwrap();
    let record = project.binding().unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority
        .admit_authenticated_write(
            &caller,
            &original,
            scope(server, &project, &record, version),
            graphit_auth::ProjectWriteOperation::Knowledge,
        )
        .unwrap();
    let pages: Vec<_> = rows
        .iter()
        .enumerate()
        .map(
            |(index, (slug, title, body, summary))| h::AuthoredPageFields {
                slug: Some(*slug),
                title: Some(*title),
                body: Some(*body),
                kind: Some("page"),
                summary: Some(*summary),
                description: None,
                summary_truncated: false,
                source_path: "docs/role-search.md",
                physical_digest: None,
                compilation_digest: h::CompilationDigest32::from_bytes(
                    [u8::try_from(index + 1).unwrap(); 32],
                ),
                recipe: h::RecipeFields {
                    id: h::AUTHORED_RECIPE_ID,
                    parameters: Some(h::AuthoredParameters {
                        fallback_title: "fallback",
                        kind_on_absent: "page",
                        max_summary_bytes: n(65536),
                    }),
                },
                updated_at: None,
                stale_since: None,
                stale_after: None,
                resolved_wiki_targets: &[],
            },
        )
        .collect();
    let prepared = h::prepare(
        original.snapshot(),
        &pages,
        &[],
        h::KnowledgeDocumentProjectionLimits {
            max_documents: n(128),
            max_observations: n(128),
            max_rows: n(1024),
            max_field_bytes: n(65536),
            max_text_bytes: n(4 * 1048576),
        },
    )
    .unwrap();
    graphit_data::knowledge_index::publish_native_authored(
        original.snapshot(),
        &admission,
        &prepared,
        KnowledgeIndexWriteLimits {
            content: server.0.policy.knowledge.content,
            native: server.0.policy.knowledge.native,
            artifacts: server.0.task.write.artifacts,
        },
    )
    .await
    .unwrap();
}
async fn update_alpha(server: &RoleToolServer, raw: &str) {
    use graphit_data::knowledge_document_update::{
        KnowledgeDocumentAuthoredUpdate, KnowledgeDocumentReferenceUpdate,
        KnowledgeDocumentUpdateLimits, KnowledgeDocumentUpdateResult,
        update_existing_native_knowledge_documents,
    };
    use graphit_data::knowledge_search::{KnowledgeSearchRead, capture_native_knowledge_search};
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let selected = project_snapshot(server);
    let (paths, id, version) = &selected.projects[0];
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.task.scope, original.snapshot())
            .unwrap();
    let record = project.binding().unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let read = authority
        .admit_authenticated_read(
            &caller,
            &original,
            scope(server, &project, &record, version),
            graphit_auth::ProjectReadModule::Knowledge,
        )
        .unwrap();
    let write = authority
        .admit_authenticated_write(
            &caller,
            &original,
            scope(server, &project, &record, version),
            graphit_auth::ProjectWriteOperation::Knowledge,
        )
        .unwrap();
    let KnowledgeSearchRead::Published(capture) =
        capture_native_knowledge_search(original.snapshot(), &read, server.0.policy.knowledge)
            .await
            .unwrap()
    else {
        panic!("existing alpha corpus required")
    };
    let fields = [h::AuthoredPageFields {
        slug: Some("alpha"),
        title: Some("Orbital alpha updated"),
        body: Some("# Órbita α\n\norbital changed α."),
        kind: Some("page"),
        summary: Some("Resumo α atualizado"),
        description: None,
        summary_truncated: false,
        source_path: "docs/role-search.md",
        physical_digest: None,
        compilation_digest: h::CompilationDigest32::from_bytes([4; 32]),
        recipe: h::RecipeFields {
            id: h::AUTHORED_RECIPE_ID,
            parameters: Some(h::AuthoredParameters {
                fallback_title: "fallback",
                kind_on_absent: "page",
                max_summary_bytes: n(65536),
            }),
        },
        updated_at: None,
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: &[],
    }];
    let rows = h::prepare(
        original.snapshot(),
        &fields,
        &[],
        h::KnowledgeDocumentProjectionLimits {
            max_documents: n(128),
            max_observations: n(128),
            max_rows: n(1024),
            max_field_bytes: n(65536),
            max_text_bytes: n(4 * 1048576),
        },
    )
    .unwrap();
    let references = [KnowledgeDocumentReferenceUpdate {
        slug: "alpha",
        authored: KnowledgeDocumentAuthoredUpdate::Preserve,
    }];
    let result = update_existing_native_knowledge_documents(
        original.snapshot(),
        &read,
        &write,
        capture.generation(),
        &rows,
        &references,
        KnowledgeDocumentUpdateLimits {
            read: server.0.policy.knowledge,
            write: KnowledgeIndexWriteLimits {
                content: server.0.policy.knowledge.content,
                native: server.0.policy.knowledge.native,
                artifacts: server.0.task.write.artifacts,
            },
        },
        |_, old, new| Ok(old == new),
    )
    .await
    .unwrap();
    assert!(matches!(
        result,
        KnowledgeDocumentUpdateResult::Updated { .. }
    ));
}

fn rows() -> [(&'static str, &'static str, &'static str, &'static str); 3] {
    [
        (
            "alpha",
            "Orbital alpha",
            "# Órbita α\n\norbital literal α.",
            "Resumo α",
        ),
        (
            "beta",
            "Orbital beta",
            "# Órbita β\n\norbital literal β.",
            "Resumo β",
        ),
        (
            "gamma",
            "Orbital gamma",
            "# Órbita γ\n\norbital literal γ.",
            "Resumo γ",
        ),
    ]
}

#[tokio::test]
async fn rerank_real_role_consumer_preserves_disabled_search_and_applies_all_six_overrides() {
    use graphit_core::config::{ConfigKey, ConfigStore};
    // This existing fixture has no AI service rules, HTTP transport or model
    // resources. Any attempted rerank admission/resolution must fail. A disabled
    // request must still complete the real authorized Native lexical query.
    for (configured, cases) in [
        (
            false,
            [(None, false), (Some(false), false), (Some(true), true)],
        ),
        (
            true,
            [(None, true), (Some(false), false), (Some(true), true)],
        ),
    ] {
        let (_temp, server, raw) = fixture();
        ConfigStore::global(&server.0.policy.source.installation)
            .unwrap()
            .set(
                ConfigKey::new("search.rerank").unwrap(),
                json!(configured),
                Duration::from_secs(5),
                &Cancellation::default(),
            )
            .unwrap();
        publish_first(&server, &raw, &rows()).await;
        let (_session, _run, reference, bearer) = credential(
            &server,
            &raw,
            Role::Scout,
            &[Permission::Read(Module::Knowledge)],
            Duration::from_secs(180),
        );
        let (uri, serving) = serve(&server).await;
        let client = connect(
            &uri,
            role_credential(&reference, &bearer, server.0.task.content),
        )
        .await;
        let mut disabled_observation = None;
        for (flag, enabled) in cases {
            let mut input = json!({"query":"orbital","page_size":1,"preview":true});
            if let Some(flag) = flag {
                input["rerank"] = json!(flag);
            }
            let page = call(
                &client,
                "graphit_knowledge_search",
                json!({"input":input}),
                enabled,
            )
            .await;
            if enabled {
                assert_eq!(page["code"], "authentication_or_permission_denied");
            } else {
                assert_eq!(page["kind"], "hits");
                assert_eq!(page["total"], 3);
                assert_eq!(page["hits"].as_array().unwrap().len(), 1);
                assert!(page["hits"][0].get("rerank_score").is_none());
                assert!(
                    page["hits"][0]["lexical_score"]
                        .as_f64()
                        .unwrap()
                        .is_finite()
                );
                assert!(page["next_cursor"].is_object());
                if let Some(previous) = &disabled_observation {
                    assert_eq!(&page, previous);
                }
                disabled_observation = Some(page);
            }
        }
        let empty = call(
            &client,
            "graphit_knowledge_search",
            json!({"input":{"query":"absentzzzz","rerank":true}}),
            false,
        )
        .await;
        assert_eq!(empty["kind"], "no_hits");
        assert_eq!(empty["total"], 0);
        assert!(empty["hits"].as_array().unwrap().is_empty());
        client.cancel().await.unwrap();
        server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn knowledge_search_native_states_pages_previews_and_schema() {
    let (_temp, server, raw) = fixture();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Knowledge)],
        Duration::from_secs(180),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let tools = client.list_all_tools().await.unwrap();
    let tool = tools
        .iter()
        .find(|tool| tool.name == "graphit_knowledge_search")
        .unwrap();
    let schema = serde_json::to_value(tool).unwrap();
    assert_eq!(schema["inputSchema"]["additionalProperties"], false);
    assert_eq!(
        schema["inputSchema"]["properties"]["input"]["required"],
        json!(["query"])
    );
    assert_eq!(
        call(
            &client,
            "graphit_knowledge_search",
            json!({"input":{"query":"orbital"}}),
            false
        )
        .await["kind"],
        "no_corpus"
    );
    for input in [
        json!({"query":" "}),
        json!({"query":"orbital","page_size":0}),
        json!({"query":"orbital","cursor":null}),
        json!({"query":"orbital","mode":"hybrid"}),
        json!({"query":"orbital","page_size":101}),
        json!({"query":"orbital","top_k":-1}),
        json!({"query":"orbital","unknown":true}),
        json!({"query":"bad\0term"}),
    ] {
        assert_eq!(
            call(
                &client,
                "graphit_knowledge_search",
                json!({"input":input}),
                true
            )
            .await["code"],
            "invalid_arguments"
        );
    }
    publish_first(&server, &raw, &[]).await;
    assert_eq!(
        call(
            &client,
            "graphit_knowledge_search",
            json!({"input":{"query":"orbital"}}),
            false
        )
        .await["kind"],
        "published_empty"
    );
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    let (_hits_temp, server, raw) = fixture();
    let (_hits_session, _hits_run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Knowledge)],
        Duration::from_secs(180),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    publish_first(&server, &raw, &rows()).await;
    let empty = call(
        &client,
        "graphit_knowledge_search",
        json!({"input":{"query":"unmatchedterm"}}),
        false,
    )
    .await;
    assert_eq!(empty["kind"], "no_hits");
    assert_eq!(empty["total"], 0);
    let mut seen = std::collections::BTreeSet::new();
    let mut previous: Option<(f64, String)> = None;
    let mut cursor = None;
    loop {
        let mut input = json!({"query":"orbital","page_size":1});
        if let Some(value) = cursor.take() {
            input["cursor"] = value;
        }
        let page = call(
            &client,
            "graphit_knowledge_search",
            json!({"input":input}),
            false,
        )
        .await;
        assert_eq!(page["kind"], "hits");
        assert_eq!(page["total"], 3);
        assert_eq!(page["hits"].as_array().unwrap().len(), 1);
        let hit = &page["hits"][0];
        let score = hit["lexical_score"].as_f64().unwrap();
        assert!(score.is_finite());
        let source = &hit["focused_source"];
        let slug = source["slug"].as_str().unwrap();
        if let Some((prior, prior_slug)) = previous.take() {
            assert!(prior > score || (prior == score && prior_slug.as_str() < slug));
        }
        previous = Some((score, slug.to_owned()));
        let rows = rows();
        let (index, row) = rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.0 == slug)
            .unwrap();
        assert_eq!(
            source,
            &json!({
                "slug":row.0, "title":row.1, "source_path":"docs/role-search.md",
                "compilation_digest":h::CompilationDigest32::from_bytes([u8::try_from(index+1).unwrap();32]),
                "namespace":null, "witness":null,
            })
        );
        assert!(hit["preview"].is_null());
        assert!(hit.get("body").is_none());
        assert!(seen.insert(slug.to_owned()));
        assert!(page["generation"]["version"].as_u64().unwrap() > 0);
        if page["next_cursor"].is_null() {
            break;
        }
        cursor = Some(page["next_cursor"].clone());
    }
    assert_eq!(
        seen,
        ["alpha", "beta", "gamma"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
    let preview = call(
        &client,
        "graphit_knowledge_search",
        json!({"input":{"query":"orbital","preview":true,"page_size":3}}),
        false,
    )
    .await;
    for hit in preview["hits"].as_array().unwrap() {
        let rows = rows();
        let row = rows
            .iter()
            .find(|row| row.0 == hit["focused_source"]["slug"])
            .unwrap();
        assert_eq!(hit["preview"]["text"], row.3);
        assert_eq!(hit["preview"]["source"], "summary");
        assert!(hit["preview"]["range"]["end"].as_u64().unwrap() > 0);
    }
    let cut = call(
        &client,
        "graphit_knowledge_search",
        json!({"input":{"query":"orbital","top_k":2,"page_size":3}}),
        false,
    )
    .await;
    assert_eq!(cut["total"], 2);
    assert_eq!(cut["hits"].as_array().unwrap().len(), 2);
    assert!(cut["next_cursor"].is_null());
    assert!(matches!(
        RoleDefinition::tool_owned(
            Role::Scribe,
            project_snapshot(&server).projects[0].0.brand(),
            server.0.task.binding,
            &[Permission::Read(Module::Knowledge)],
            [37; 32],
            IssuerLimits {
                max_permissions: 64,
                max_scope_bytes: 65536,
                max_parent_bearer_bytes: 4096,
            },
        ),
        Err(graphit_auth::AuthError::Denied)
    ));
    {
        let role = Role::Tracker;
        let (_session, _run, reference, bearer) = credential(
            &server,
            &raw,
            role,
            &[Permission::Read(Module::Knowledge)],
            Duration::from_secs(180),
        );
        let recipient = connect(
            &uri,
            role_credential(&reference, &bearer, server.0.task.content),
        )
        .await;
        assert!(
            recipient
                .list_all_tools()
                .await
                .unwrap()
                .iter()
                .any(|tool| tool.name == "graphit_knowledge_search")
        );
        let result = call(
            &recipient,
            "graphit_knowledge_search",
            json!({"input":{"query":"orbital","preview":true,"page_size":3}}),
            false,
        )
        .await;
        assert_eq!(result, preview);
        recipient.cancel().await.unwrap();
    }
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn knowledge_search_role_scope_cursor_generation_and_revocation() {
    let (_temp, server, raw) = fixture();
    publish_first(&server, &raw, &rows()).await;
    let permissions = [Permission::Read(Module::Knowledge)];
    let (session, run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &permissions,
        Duration::from_secs(180),
    );
    let (_other_session, _other_run, other_ref, other_bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &permissions,
        Duration::from_secs(180),
    );
    let (_denied_session, _denied_run, denied_ref, denied_bearer) = credential(
        &server,
        &raw,
        Role::Scribe,
        &[Permission::Read(Module::Task)],
        Duration::from_secs(180),
    );
    let projects = Arc::clone(&project_snapshot(&server).projects);
    let (_peer_session, _peer_run, peer_ref, peer_bearer) = credential_at(
        &server,
        &raw,
        1,
        Role::Tracker,
        &permissions,
        Duration::from_secs(180),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let other = connect(
        &uri,
        role_credential(&other_ref, &other_bearer, server.0.task.content),
    )
    .await;
    let denied = connect(
        &uri,
        role_credential(&denied_ref, &denied_bearer, server.0.task.content),
    )
    .await;
    let peer = connect_at(
        &uri,
        role_credential(&peer_ref, &peer_bearer, server.0.task.content),
        Some(json!({"project_id":projects[1].1})),
    )
    .await;
    assert!(
        !denied
            .list_all_tools()
            .await
            .unwrap()
            .iter()
            .any(|tool| tool.name == "graphit_knowledge_search")
    );
    assert_eq!(
        call(
            &denied,
            "graphit_knowledge_search",
            json!({"input":{"query":"orbital"}}),
            true
        )
        .await["code"],
        "authentication_or_permission_denied"
    );
    let page = call(
        &client,
        "graphit_knowledge_search",
        json!({"input":{"query":"orbital","page_size":1}}),
        false,
    )
    .await;
    let cursor = page["next_cursor"].clone();
    assert!(cursor.is_object());
    assert_eq!(
        call(
            &other,
            "graphit_knowledge_search",
            json!({"input":{"query":"orbital","page_size":1,"cursor":cursor}}),
            true
        )
        .await["code"],
        "invalid_cursor"
    );
    assert_eq!(
        call(
            &client,
            "graphit_knowledge_search",
            json!({"input":{"query":"alpha","page_size":1,"cursor":cursor}}),
            true
        )
        .await["code"],
        "invalid_cursor"
    );
    assert_eq!(
        call(
            &client,
            "graphit_knowledge_search",
            json!({"selection":{"project_id":projects[1].1},"input":{"query":"orbital"}}),
            true
        )
        .await["code"],
        "authentication_or_permission_denied"
    );
    assert_eq!(
        call(
            &peer,
            "graphit_knowledge_search",
            json!({"input":{"query":"orbital"}}),
            false
        )
        .await["kind"],
        "no_corpus"
    );
    assert_eq!(
        call(
            &peer,
            "graphit_knowledge_search",
            json!({"input":{"query":"orbital","page_size":1,"cursor":cursor}}),
            true
        )
        .await["code"],
        "generation_changed"
    );
    assert_eq!(
        call(
            &client,
            "graphit_knowledge_search",
            json!({"selection":{"project_version":"unknown-version"},"input":{"query":"orbital"}}),
            true
        )
        .await["code"],
        "selection_unavailable"
    );
    update_alpha(&server, &raw).await;
    assert_eq!(
        call(
            &client,
            "graphit_knowledge_search",
            json!({"input":{"query":"orbital","page_size":1,"cursor":cursor}}),
            true
        )
        .await["code"],
        "generation_changed"
    );
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(&raw, &original).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (started, observed) = std::sync::mpsc::channel();
    let (release, waiting) = oneshot::channel();
    *server.0.before_delivery.lock().unwrap() = Some((started, waiting));
    let operation = client.call_tool(
        CallToolRequestParams::new("graphit_knowledge_search").with_arguments(
            json!({"ai_optimized":false,"input":{"query":"orbital"}})
                .as_object()
                .unwrap()
                .clone(),
        ),
    );
    let revoke = async {
        tokio::task::spawn_blocking(move || {
            observed.recv_timeout(Duration::from_secs(10)).unwrap()
        })
        .await
        .unwrap();
        issuer.close_run(&parent, &run).unwrap();
        release.send(()).unwrap();
    };
    let (reply, ()) = tokio::join!(operation, revoke);
    let reply = reply.unwrap();
    assert_eq!(reply.is_error, Some(true));
    let revoked: Value = typed_value(&reply);
    assert_eq!(revoked["code"], "authentication_or_permission_denied");
    assert!(revoked.get("hits").is_none());
    assert_eq!(
        call(
            &other,
            "graphit_knowledge_search",
            json!({"input":{"query":"orbital"}}),
            false
        )
        .await["total"],
        3
    );
    server
        .0
        .live_projects
        .replace(Arc::new(vec![projects[1].clone()]), 0)
        .unwrap();
    assert_eq!(
        call(
            &other,
            "graphit_knowledge_search",
            json!({"selection":{"project_id":projects[0].1},"input":{"query":"orbital"}}),
            true
        )
        .await["code"],
        "selection_unavailable"
    );
    assert_eq!(
        call(
            &peer,
            "graphit_knowledge_search",
            json!({"input":{"query":"orbital"}}),
            false
        )
        .await["kind"],
        "no_corpus"
    );
    client.cancel().await.unwrap();
    other.cancel().await.unwrap();
    denied.cancel().await.unwrap();
    peer.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn knowledge_search_preview_failure_is_not_hits_and_runtime_is_not_role() {
    let (_temp, mut server, raw) = fixture();
    Arc::get_mut(&mut server.0)
        .unwrap()
        .task
        .content
        .max_input_bytes = 65536;
    let body = format!("orbital {}", "x".repeat(65000));
    let summary = "s".repeat(1024);
    publish_first(
        &server,
        &raw,
        &[("large", "Orbital large", &body, &summary)],
    )
    .await;
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Tracker,
        &[Permission::Read(Module::Knowledge)],
        Duration::from_secs(180),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    assert_eq!(
        call(
            &client,
            "graphit_knowledge_search",
            json!({"input":{"query":"orbital","preview":false}}),
            false
        )
        .await["kind"],
        "hits"
    );
    let failed = call(
        &client,
        "graphit_knowledge_search",
        json!({"input":{"query":"orbital","preview":true}}),
        true,
    )
    .await;
    assert_eq!(failed["code"], "knowledge_preview_failed");
    assert!(failed.get("hits").is_none());
    let oversized = server
        .call(
            "graphit_knowledge_search".into(),
            serde_json::from_value(
                json!({"input":{"query":"x".repeat(server.0.task.content.max_scalar_bytes + 1)}}),
            )
            .unwrap(),
            role_credential(&reference, &bearer, server.0.task.content),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(oversized.is_error, Some(true));
    let oversized: Value = typed_value(&oversized);
    assert_eq!(oversized["code"], "invalid_arguments");
    let args = serde_json::from_value(json!({"input":{"query":"orbital"}})).unwrap();
    let denied = server
        .call(
            "graphit_knowledge_search".into(),
            args,
            Credential {
                role_binding: None,
                bearer: Zeroizing::new(raw),
                origin: Origin::Runtime,
            },
            CancellationToken::new(),
        )
        .await;
    assert_eq!(denied.is_error, Some(true));
    let denied: Value = typed_value(&denied);
    assert_eq!(denied["code"], "role_credential_required");
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn knowledge_search_cancel_deadline_and_owned_shutdown() {
    use rmcp::model::{CallToolRequest, ClientRequest};
    for deadline in [false, true] {
        let (_temp, mut server, raw) = fixture();
        publish_first(&server, &raw, &rows()).await;
        if deadline {
            Arc::get_mut(&mut server.0).unwrap().task.timeout = Duration::from_secs(5);
        }
        let (_session, _run, reference, bearer) = credential(
            &server,
            &raw,
            Role::Scout,
            &[Permission::Read(Module::Knowledge)],
            Duration::from_secs(180),
        );
        let (uri, serving) = serve(&server).await;
        let client = connect(
            &uri,
            role_credential(&reference, &bearer, server.0.task.content),
        )
        .await;
        let (started, observed) = std::sync::mpsc::channel();
        let (release, waiting) = oneshot::channel();
        *server.0.before_delivery.lock().unwrap() = Some((started, waiting));
        let params = CallToolRequestParams::new("graphit_knowledge_search").with_arguments(
            json!({"ai_optimized":false,"input":{"query":"orbital"}})
                .as_object()
                .unwrap()
                .clone(),
        );
        if deadline {
            let operation = client.call_tool(params);
            let observe = tokio::task::spawn_blocking(move || {
                observed.recv_timeout(Duration::from_secs(10)).unwrap()
            });
            let (reply, cancellation) = tokio::join!(operation, observe);
            let reply = reply.unwrap();
            assert_eq!(reply.is_error, Some(true));
            let value: Value = typed_value(&reply);
            assert_eq!(value["code"], "deadline");
            assert!(cancellation.unwrap().is_cancelled());
        } else {
            let pending = client
                .send_cancellable_request(
                    ClientRequest::CallToolRequest(CallToolRequest::new(params)),
                    Default::default(),
                )
                .await
                .unwrap();
            let cancellation = tokio::task::spawn_blocking(move || {
                observed.recv_timeout(Duration::from_secs(10)).unwrap()
            })
            .await
            .unwrap();
            pending.cancel(None).await.unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                while !cancellation.is_cancelled() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        }
        release.send(()).unwrap();
        assert_eq!(
            call(
                &client,
                "graphit_knowledge_search",
                json!({"input":{"query":"orbital"}}),
                false
            )
            .await["kind"],
            "hits"
        );
        client.cancel().await.unwrap();
        server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
}
