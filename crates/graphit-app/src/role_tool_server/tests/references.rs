use super::*;
use graphit_data::{
    knowledge_document_projection as h,
    knowledge_document_update::{
        KnowledgeDocumentAuthoredUpdate, KnowledgeDocumentReferenceUpdate,
    },
    knowledge_first_publication::publish_native_authored_with_reference_updates,
    knowledge_references::{
        KnowledgeAuthoredReference, KnowledgeReferenceScope, KnowledgeReferenceTargetKind,
    },
};

async fn task_with_references(server: &RoleToolServer, raw: &str) -> Value {
    let runtime = || Credential {
        role_binding: None,
        bearer: Zeroizing::new(raw.to_owned()),
        origin: Origin::Runtime,
    };
    let arguments = |input| serde_json::from_value::<Arguments>(json!({"input":input})).unwrap();
    let session = server.0.tasks.call(
        arguments(json!({"idempotency_key":"references-session","title":"Actual references","description":"Native source","strategy":"SDK"})),
        "create_session", true, runtime(), CancellationToken::new(),
    ).await.unwrap_or_else(|_| panic!("actual Task session publication failed"));
    let session_id = &session["result"]["id"];
    let task = server.0.tasks.call(
        arguments(json!({"idempotency_key":"references-task","session_id":session_id,"title":"Reference source","description":"Authored edges","task_type":"task","priority":1,
            "references":[{"target":{"type":"session","id":session_id},"relation":"supports"},
                {"target":{"type":"memory","id":"mem-foreign","scope":"user","scope_id":"other-owner"},"relation":"uses"}],
            "checks":[{"kind":"acceptance","text":"Read the actual source edges"},{"kind":"test","text":"Exercise Role MCP"}]})),
        "create_task", true, runtime(), CancellationToken::new(),
    ).await.unwrap_or_else(|_| panic!("actual Task reference publication failed"));
    task["result"]["specification"]["id"].clone()
}

pub(super) async fn knowledge_with_references(server: &RoleToolServer, raw: &str) {
    let cancellation = Cancellation::default();
    let original = request(&cancellation, server.0.task.content);
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let snapshot = project_snapshot(server);
    let (paths, id, version) = &snapshot.projects[0];
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.task.scope, original.snapshot())
            .unwrap();
    let binding = project.binding().unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let write = authority
        .admit_authenticated_write(
            &caller,
            &original,
            scope(server, &project, &binding, version),
            graphit_auth::ProjectWriteOperation::Knowledge,
        )
        .unwrap();
    let fields: Vec<_> = ["alpha", "beta", "gamma"]
        .into_iter()
        .enumerate()
        .map(|(index, slug)| h::AuthoredPageFields {
            slug: Some(slug),
            title: Some(slug),
            body: Some("# Deployment policy\n\nOnly maintainers archive projects.\n"),
            kind: Some("page"),
            summary: Some("Deployment policy for maintainers."),
            description: None,
            summary_truncated: false,
            source_path: "docs/references.md",
            physical_digest: None,
            compilation_digest: h::CompilationDigest32::from_bytes(
                [u8::try_from(index + 1).unwrap(); 32],
            ),
            recipe: h::RecipeFields {
                id: h::AUTHORED_RECIPE_ID,
                parameters: Some(h::AuthoredParameters {
                    fallback_title: "fallback",
                    kind_on_absent: "page",
                    max_summary_bytes: n(256),
                }),
            },
            updated_at: None,
            stale_since: None,
            stale_after: None,
            resolved_wiki_targets: &[],
        })
        .collect();
    let prepared = h::prepare(
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
    let edges = [
        KnowledgeAuthoredReference {
            target_kind: KnowledgeReferenceTargetKind::Knowledge,
            scope: KnowledgeReferenceScope::Project(*id),
            id: "beta",
            context: None,
            relation: "supports",
            field: None,
        },
        KnowledgeAuthoredReference {
            target_kind: KnowledgeReferenceTargetKind::Memory,
            scope: KnowledgeReferenceScope::User("other-owner"),
            id: "mem-foreign",
            context: None,
            relation: "uses",
            field: None,
        },
    ];
    publish_native_authored_with_reference_updates(
        original.snapshot(),
        &write,
        &prepared,
        &[
            KnowledgeDocumentReferenceUpdate {
                slug: "alpha",
                authored: KnowledgeDocumentAuthoredUpdate::Replace(&edges),
            },
            KnowledgeDocumentReferenceUpdate {
                slug: "beta",
                authored: KnowledgeDocumentAuthoredUpdate::Replace(&[]),
            },
            KnowledgeDocumentReferenceUpdate {
                slug: "gamma",
                authored: KnowledgeDocumentAuthoredUpdate::Preserve,
            },
        ],
        KnowledgeIndexWriteLimits {
            content: server.0.policy.knowledge.content,
            native: server.0.policy.knowledge.native,
            artifacts: server.0.task.write.artifacts,
        },
    )
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_task_references_native_pages_keep_foreign_targets_unresolved() {
    let (_temp, server, raw) = fixture();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Tracker,
        &[Permission::Read(Module::Task)],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let tools = client.list_all_tools().await.unwrap();
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "graphit_task_references")
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool.name == "graphit_knowledge_references")
    );
    assert_eq!(
        call(
            &client,
            "graphit_task_references",
            json!({"input":{"focus":{"task":graphit_records::task_spec::TaskId::new(std::num::NonZeroU128::MIN)},"page_size":1}}),
            false
        )
        .await,
        "no_corpus"
    );
    let id = task_with_references(&server, &raw).await;
    let mut cursor = Value::Null;
    let mut edges = Vec::new();
    let mut reached_eof = false;
    // The structural session edge and two authored edges, plus an optional EOF page.
    for _ in 0..4 {
        let value = call(
            &client,
            "graphit_task_references",
            json!({"input":{"focus":{"task":id},"page_size":1,"outgoing_cursor":cursor}}),
            false,
        )
        .await;
        let page = &value["published"]["outgoing"];
        let rows = page["results"].as_array().unwrap();
        assert!(rows.len() <= 1);
        for row in rows {
            assert!(!edges.contains(row), "reference repeated across pages");
        }
        edges.extend(rows.iter().cloned());
        assert!(edges.len() <= 3, "the fixture has exactly three references");
        let next = page["next_cursor"].clone();
        if next.is_null() {
            reached_eof = true;
            break;
        }
        assert_ne!(next, cursor, "reference cursor must advance");
        cursor = next;
    }
    assert!(reached_eof, "the fixture traversal must reach EOF");
    assert_eq!(edges.len(), 3);
    assert_eq!(edges[0]["reference"]["relation"], "session");
    assert!(
        edges
            .iter()
            .any(|edge| edge["reference"]["relation"] == "supports")
    );
    let foreign = edges
        .iter()
        .find(|edge| edge["reference"]["id"] == "mem-foreign")
        .unwrap();
    assert_eq!(foreign["target_resolution"], "unresolved");
    assert_eq!(foreign["reference"]["scope"]["user"], "other-owner");
    let peer = project_snapshot(&server).projects[1].clone();
    assert_eq!(call(&client,"graphit_task_references",json!({"selection":{"project_id":peer.1,"project_version":peer.2.version()},"input":{"focus":{"task":id},"page_size":1}}),true).await["code"],"authentication_or_permission_denied");
    for input in [
        json!({"focus":{"task":id},"page_size":0}),
        json!({"focus":{"task":id},"page_size":101}),
        json!({"focus":{"task":id},"page_size":1,"resolve_foreign":true}),
    ] {
        call(
            &client,
            "graphit_task_references",
            json!({"input":input}),
            true,
        )
        .await;
    }
    assert_eq!(
        call(
            &client,
            "graphit_knowledge_references",
            json!({"input":{"outgoing":"alpha","page_size":1}}),
            true
        )
        .await["code"],
        "authentication_or_permission_denied"
    );
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_knowledge_references_native_states_cursors_and_literal_backlinks() {
    let (_temp, server, raw) = fixture();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Knowledge)],
        Duration::from_secs(90),
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
            "graphit_knowledge_references",
            json!({"input":{"outgoing":"alpha","page_size":1}}),
            false
        )
        .await,
        "no_corpus"
    );
    knowledge_with_references(&server, &raw).await;
    let first = call(
        &client,
        "graphit_knowledge_references",
        json!({"input":{"outgoing":"alpha","page_size":1}}),
        false,
    )
    .await;
    let page = &first["published"]["read"]["outgoing"];
    assert_eq!(page["total"], 2);
    assert_eq!(page["results"].as_array().unwrap().len(), 1);
    let cursor = &page["next_cursor"];
    assert!(cursor.is_object());
    let second = call(
        &client,
        "graphit_knowledge_references",
        json!({"input":{"outgoing":"alpha","page_size":1,"outgoing_cursor":cursor}}),
        false,
    )
    .await;
    let rows = second["published"]["read"]["outgoing"]["results"]
        .as_array()
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["target_observation"], "unresolved");
    assert_eq!(
        rows[0]["payload"]["authored"]["target"]["namespace"]["scope"]["user"],
        "other-owner"
    );
    assert_eq!(
        call(
            &client,
            "graphit_knowledge_references",
            json!({"input":{"outgoing":"beta","page_size":1,"outgoing_cursor":cursor}}),
            true
        )
        .await["code"],
        "invalid_cursor"
    );
    let empty = call(
        &client,
        "graphit_knowledge_references",
        json!({"input":{"outgoing":"beta","page_size":1}}),
        false,
    )
    .await;
    assert_eq!(empty["published"]["read"]["outgoing"]["total"], 0);
    assert_eq!(empty["published"]["read"]["outgoing"]["complete"], true);
    let incoming = call(&client,"graphit_knowledge_references",json!({"input":{"incoming":{"target_kind":"memory","scope":{"user":"other-owner"},"id":"mem-foreign"},"page_size":1}}),false).await;
    assert_eq!(
        incoming["published"]["read"]["incoming"]["results"][0]["source"]["slug"],
        "alpha"
    );
    assert_eq!(
        incoming["published"]["read"]["coverage"]["knowledge_source_complete"],
        false
    );
    assert_eq!(
        call(
            &client,
            "graphit_knowledge_references",
            json!({"input":{"outgoing":"absent","page_size":1}}),
            true
        )
        .await["code"],
        "unknown_source"
    );
    for input in [
        json!({"page_size":0}),
        json!({"outgoing":"alpha","page_size":1,"resolve_foreign":true}),
    ] {
        call(
            &client,
            "graphit_knowledge_references",
            json!({"input":input}),
            true,
        )
        .await;
    }
    assert_eq!(
        call(
            &client,
            "graphit_task_references",
            json!({"input":{"focus":{"task":graphit_records::task_spec::TaskId::new(std::num::NonZeroU128::MIN)},"page_size":1}}),
            true
        )
        .await["code"],
        "authentication_or_permission_denied"
    );
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
