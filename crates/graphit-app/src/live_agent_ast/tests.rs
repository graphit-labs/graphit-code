//! Registered under browser_auth::tests after the shared build gate.
//! Reuses its real OAuth issuer and the existing Source/Graph SDK fixtures.
use super::*;
use crate::{
    ast_source::SourceParameters,
    graph_server::{
        GraphServer,
        tests::{web_fixture, web_import_fixture},
    },
    knowledge_live::ArtifactSelection,
    live_agent_ast::{self as ast, AstSelection},
};
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture,
    broker::project_access::{BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead},
};
use graphit_core::{artifact_store::CapturedPublicationScope, file_lock::Cancellation};
use graphit_hub::policy::RequestContext;

async fn open(
    source: &mut crate::source_server::test_fixture::Fixture,
    graphs: &GraphServer,
) -> Fixture {
    crate::source_server::replace_fixture_graph(source, graphs.clone());
    Fixture::with_graphs(policy(), (source.server.clone(), source.raw.clone())).await
}
fn parameters(value: Value) -> SourceParameters {
    serde_json::from_value(value).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_ast_project_capture_without_graph_or_index_reads_only_authorized_disk_source() {
    let (mut source, graphs, destination) = web_fixture().await;
    std::fs::remove_dir_all(destination).unwrap();
    std::fs::write(
        source.projects[0].0.root().join("arbitrary.unsupported"),
        "one\ntwo\nthree\nfour\nfive",
    )
    .unwrap();
    let f = open(&mut source, &graphs).await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
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
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        source.server.policy().content,
        &cancellation,
        Instant::now() + Duration::from_secs(40),
    )
    .unwrap();
    let bearer = zeroize::Zeroizing::new(entry.credentials.lock().await.access_token().to_owned());
    let flow = f
        .service
        .0
        .flow(entry.provider)
        .unwrap_or_else(|_| panic!("registered Web flow fixture"));
    let (provider, verifier) = &f.service.0.providers[entry.provider];
    let caller = flow
        .verify_access(
            &entry.registered,
            &bearer,
            BrokerRequestContext::new(original.snapshot()),
        )
        .await
        .unwrap();
    let project = &source.projects[0];
    let scope = CapturedPublicationScope::project(
        &project.0,
        project.1,
        source.server.policy().scope,
        original.snapshot(),
    )
    .unwrap();
    let binding = scope.binding().unwrap();
    let authority = ProjectRecordAuthority::new(source.auth.issuer());
    let backend = task::backend(&format!("{}/v1/project/access/authorize", f.issuer.origin));
    let read = authority
        .admit_broker_read(BrokerRecordRead {
            caller: BrokerProjectCaller::Web(&caller),
            verifier,
            backend: &backend,
            original: &original,
            scope: BrokerProjectScope {
                installation: source.auth.installation(),
                project: ProjectCapture {
                    scope: ScopeCapture {
                        live: &scope,
                        record: &binding,
                    },
                    version: &project.2,
                },
                providers: &[],
            },
            provider,
            module: ProjectReadModule::Ast,
        })
        .await
        .unwrap();
    let selection = ArtifactSelection {
        source: "project".into(),
        project_id: Some(project.1.to_string()),
        instance_id: None,
        project_kind: None,
        artifact_type: "ast".into(),
        id: project.1.to_string(),
        version: project.2.version().into(),
    };
    let selected = AstSelection {
        selection: &selection,
        project_index: Some(0),
        imported: None,
    };
    let capture = ast::capture_project(
        &selected,
        project,
        &graphs,
        &source.server,
        &read,
        source.auth.installation(),
        &original,
    )
    .await
    .unwrap();
    assert_eq!(
        capture.schema().await.unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    for (options, expected) in [
        (json!({"head":3,"tail":1}), "three"),
        (json!({"start_line":2,"end_line":4}), "two\nthree\nfour"),
        (
            json!({"tail":2,"line_numbers":true}),
            "   4: four\n   5: five",
        ),
        (
            json!({"pattern":"THREE","before":1,"after":1}),
            "     2: two\n>    3: three\n     4: four",
        ),
    ] {
        let mut input = json!({"path":"arbitrary.unsupported"});
        input
            .as_object_mut()
            .unwrap()
            .extend(options.as_object().unwrap().clone());
        let page = serde_json::to_value(capture.source(&parameters(input)).await.unwrap()).unwrap();
        assert_eq!(page["origin"], "physical");
        assert_eq!(page["selection"]["source"], expected);
    }
    assert!(
        capture
            .source(&parameters(json!({"path":"../outside"})))
            .await
            .is_err()
    );
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    f.service.shutdown().await.unwrap();
    graphs.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_ast_imported_source_keeps_raw_pin_and_rejects_dataset_replacement() {
    let (mut source, graphs) = web_import_fixture().await;
    std::fs::write(
        source.projects[0].0.root().join("not-in-copy.unsupported"),
        "must not be read from PROJECT",
    )
    .unwrap();
    let f = open(&mut source, &graphs).await;
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
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        source.server.policy().content,
        &cancellation,
        Instant::now() + Duration::from_secs(40),
    )
    .unwrap();
    let bearer = zeroize::Zeroizing::new(entry.credentials.lock().await.access_token().to_owned());
    let flow = f
        .service
        .0
        .flow(entry.provider)
        .unwrap_or_else(|_| panic!("registered Web flow fixture"));
    let (provider, verifier) = &f.service.0.providers[entry.provider];
    let caller = flow
        .verify_access(
            &entry.registered,
            &bearer,
            BrokerRequestContext::new(original.snapshot()),
        )
        .await
        .unwrap();
    let read = source
        .auth
        .issuer()
        .admit_web_imported_ast_read(
            &caller,
            verifier,
            &original,
            provider,
            source.auth.installation(),
        )
        .unwrap();
    let policy = source.server.policy();
    let document = policy.registry.load().unwrap();
    let artifact = document
        .entries()
        .find(|(kind, _, record)| {
            *kind == graphit_hub::ArtifactType::Ast
                && record
                    .installations
                    .iter()
                    .any(|v| v.original_source.version().as_str() == "web-import")
        })
        .unwrap()
        .2;
    let installed = artifact
        .installations
        .iter()
        .find(|v| v.original_source.version().as_str() == "web-import")
        .unwrap();
    let selection = crate::live_agent_catalog::selection(&installed.original_source);
    let selected = AstSelection {
        selection: &selection,
        project_index: None,
        imported: Some((
            artifact.id.clone(),
            installed.original_source.version().clone(),
        )),
    };
    // Use the admitted SDK to locate the actual Raw dataset, rather than
    // guessing the installed layout or adding a production getter for a test.
    let (id, version) = selected.imported.as_ref().unwrap();
    let raw_path = {
        let graph = graphit_ast::imported_graph::open_imported_graph_admitted(
            &read,
            graphit_ast::imported_graph::ImportedGraphInput {
                registry: &policy.registry,
                id,
                version,
                installation: read.installation(),
                file_policy: &policy.file,
                read_limits: policy.read,
                graph_policy: policy.native_install.graph,
                directory_limits: policy.index,
                request: &original,
            },
        )
        .unwrap();
        graph
            .reader()
            .unwrap()
            .source_reader(original.snapshot())
            .unwrap()
            .source_path(original.snapshot())
            .unwrap()
            .join("source.lance")
    };
    let capture = ast::capture_local_imported(&selected, &graphs, &source.server, &read)
        .await
        .unwrap();
    let page = serde_json::to_value(
        capture
            .source(&parameters(
                json!({"path":"main.rs","entity":"hello","entity_type":"Function","head":2}),
            ))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(page["origin"], "indexed");
    assert_eq!(page["selection"]["source"], "fn hello\nbody");
    assert!(
        capture
            .source(&parameters(json!({"path":"not-in-copy.unsupported"})))
            .await
            .is_err()
    );
    let schema = graphit_ast::indexed_source::source_schema();
    let store = graphit_data::lance_module_store::LanceModuleStore::open(
        raw_path.to_str().unwrap(),
        Arc::clone(&schema),
    )
    .await
    .unwrap();
    let batch = arrow_array::RecordBatch::try_new(
        schema,
        vec![
            Arc::new(arrow_array::StringArray::from(vec!["main.rs"])),
            Arc::new(arrow_array::StringArray::from(vec!["replacement"])),
        ],
    )
    .unwrap();
    store
        .snapshot()
        .prepare_replace(vec![batch])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    assert!(capture.recheck().is_err());
    assert!(
        capture
            .source(&parameters(json!({"path":"main.rs"})))
            .await
            .is_err()
    );
    f.service.shutdown().await.unwrap();
    graphs.shutdown().await.unwrap();
}
