//! Real Task Document/Query consumers of the provisioned local GGUF service.
use super::*;
use futures_util::FutureExt;
use graphit_daemon::supervisor::ProjectContext;
use graphit_records::task_store::{
    NativeTaskStore, TASK_DOCUMENT_RECIPE, TaskEmbeddingCorpusRead, TaskEmbeddingIdentity,
};
use std::panic::AssertUnwindSafe;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires provisioned GRAPHIT_EMBEDDINGGEMMA2_MODEL BF16 GGUF and GRAPHIT_TEST_MODEL_SERVER"]
async fn task_local_gguf_document_query_preserves_recipe_and_reuses_vectors() {
    let mut fixture = Fixture::new();
    fixture.projects.truncate(1);
    // Keep the private model inode alive until both the jobs and Source stop.
    let _bundle = super::native_gguf_source::install_gguf(&fixture);
    let global = ConfigStore::global(&fixture.installation).unwrap();
    for (module, enabled) in [
        (Module::Memory, false),
        (Module::Ast, false),
        (Module::Knowledge, false),
        (Module::Task, true),
        (Module::Embedding, true),
    ] {
        global
            .set(
                module.key(),
                json!(enabled),
                Duration::from_secs(5),
                &Cancellation::default(),
            )
            .unwrap();
    }
    fixture.config = ConfigSnapshot::new(fixture.config.brand(), [global.load().unwrap()]).unwrap();
    fixture.policy = HubResourcePolicy::resolve(&fixture.config).unwrap();
    let cancel = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancel)
        .unwrap();
    let provider = active_provider(&fixture.installation, &fixture.config, &request)
        .unwrap()
        .unwrap();
    let foreground = ForegroundSource::start_with_inventory(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        &request,
        Some(&provider),
    )
    .unwrap();
    let modules = foreground
        .server
        .as_ref()
        .unwrap()
        .project_modules(&request)
        .unwrap();
    let mut jobs = foreground
        .create_memory_jobs(
            &fixture.installation,
            &fixture.config,
            Some(&provider),
            &request,
            &modules,
        )
        .unwrap()
        .expect("Task + local embedding enables the existing background resources");
    let exercise = AssertUnwindSafe(async {
        let resources = jobs
            .start(
                &fixture.installation,
                fixture.environment(),
                foreground.scope,
                foreground.native,
                NativeBinding::KiroV3,
                &request,
            )
            .await
            .unwrap()
            .unwrap();
        assert!(jobs.child.is_some());
        let local = resources
            .local
            .as_ref()
            .expect("actual local model service");
        let metadata = resources.metadata(&request).await.unwrap();
        assert_eq!(metadata.model, crate::model_provision::MODEL_ID);
        assert_eq!(metadata.revision, crate::model_provision::MODEL_REVISION);
        assert_eq!(metadata.dimensions, 768);
        assert_eq!(metadata.vector_id, local.expected.vector_id());
        let identity = TaskEmbeddingIdentity::new(
            metadata.vector_id,
            NonZeroU32::new(u32::try_from(metadata.dimensions).unwrap()).unwrap(),
            TASK_DOCUMENT_RECIPE.into(),
        )
        .unwrap();
        assert_eq!(identity.recipe(), TASK_DOCUMENT_RECIPE);
        assert_eq!(identity.vector_id(), &metadata.vector_id);
        assert_eq!(identity.dimensions().get(), 768);

        let server = foreground.server.as_ref().unwrap().task_server().unwrap();
        server
            .attach_memory_model(Some(Arc::clone(&resources)))
            .unwrap();
        let runtime = foreground.auth.runtime().check(request.snapshot()).unwrap();
        let raw = zeroize::Zeroizing::new(
            std::str::from_utf8(runtime.secret(request.snapshot()).unwrap())
                .unwrap()
                .to_owned(),
        );
        let credential = || crate::task_server::request::Credential {
            role_binding: None,
            bearer: raw.clone(),
            origin: crate::task_server::request::Origin::Runtime,
        };
        let invoke = |operation, write, input| {
            server.call(
                serde_json::from_value(json!({"input": input})).unwrap(),
                operation,
                write,
                credential(),
                tokio_util::sync::CancellationToken::new(),
            )
        };
        let session = invoke(
            "create_session",
            true,
            json!({"idempotency_key":"local-gguf-session","title":"Local Task search",
                "description":"Qualify the local Task producer and consumer",
                "strategy":"Publish documents progressively"}),
        )
        .await
        .unwrap_or_else(|_| panic!("create Task session"));
        let create = |key, title, description| {
            invoke(
                "create_task",
                true,
                json!({"idempotency_key":key,"session_id":session["result"]["id"],
                    "title":title,"description":description,"task_type":"TASK","priority":1,
                    "checks":[{"kind":"acceptance","text":"Preserve published records"},
                        {"kind":"test","text":"Search the current Task corpus"}]}),
            )
        };
        let first = create(
            "local-gguf-first",
            "Recover publication",
            "Resume an interrupted publication from committed records",
        )
        .await
        .unwrap_or_else(|_| panic!("create first Task"));
        let first_id = first["result"]["specification"]["id"].clone();

        let (project, id, version) = &foreground.projects.as_ref().unwrap()[0];
        let selected = ProjectContext {
            id: *id,
            version: version.clone(),
        };
        let captured =
            CapturedPublicationScope::project(project, *id, foreground.scope, request.snapshot())
                .unwrap();
        let binding = captured.binding().unwrap();
        let issuer = foreground.auth.issuer();
        let caller = issuer.authenticate_runtime(&raw, &request).unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        let read = authority
            .admit_authenticated_read(
                &caller,
                &request,
                ScopeInput {
                    installation: foreground.auth.installation(),
                    project: Some(ProjectCapture {
                        scope: ScopeCapture {
                            live: &captured,
                            record: &binding,
                        },
                        version,
                    }),
                    providers: &[],
                    binding: NativeBinding::KiroV3,
                    global_version: "",
                },
                ProjectReadModule::Task,
            )
            .unwrap();
        let policy = crate::foreground_tasks::task_policy(
            &fixture.policy,
            foreground.scope,
            foreground.native,
            NativeBinding::KiroV3,
        )
        .unwrap();
        let pending = || async {
            let TaskEmbeddingCorpusRead::Published(mut corpus) =
                NativeTaskStore::capture_embeddings(
                    request.snapshot(),
                    &read,
                    identity.clone(),
                    policy.read,
                )
                .await
                .unwrap()
            else {
                panic!("Task corpus must be published")
            };
            let generation = corpus.generation().clone();
            let documents = corpus
                .next_documents(request.snapshot(), &read, NonZeroUsize::new(16).unwrap())
                .await
                .unwrap()
                .unwrap_or_default();
            assert!(
                corpus
                    .next_documents(request.snapshot(), &read, NonZeroUsize::new(16).unwrap())
                    .await
                    .unwrap()
                    .is_none()
            );
            (generation, documents)
        };
        let (_, documents) = pending().await;
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].identity(), &identity);
        assert_eq!(
            serde_json::to_value(documents[0].task_id()).unwrap(),
            first_id
        );
        assert!(documents[0].text().contains("Recover publication"));

        // The production indexer sends Document input through the actual local RPC.
        resources
            .index_tasks(project, &selected, &cancel)
            .await
            .unwrap();
        let second = create(
            "local-gguf-second",
            "Orchid lexeme",
            "Keep this later Task discoverable before its document has a vector",
        )
        .await
        .unwrap_or_else(|_| panic!("create uncovered Task"));
        let second_id = second["result"]["specification"]["id"].clone();
        let (_, documents) = pending().await;
        assert_eq!(documents.len(), 1, "the first document vector is reused");
        assert_eq!(documents[0].identity(), &identity);
        assert_eq!(
            serde_json::to_value(documents[0].task_id()).unwrap(),
            second_id
        );

        // Authenticated search resolves the same model/Task recipe and sends Query input.
        let semantic = invoke(
            "search",
            false,
            json!({"mode":"semantic","text":"Recover publication","filter":{},
                "page_size":10,"top_k":10}),
        )
        .await
        .unwrap_or_else(|_| panic!("local Task semantic query"));
        let hits = semantic["result"]["Published"]["results"]
            .as_array()
            .unwrap();
        assert_eq!(hits.len(), 1, "only the covered Task has a vector");
        assert_eq!(hits[0]["task"]["id"], first_id);
        let hybrid = invoke(
            "search",
            false,
            json!({"mode":"hybrid","text":"Orchid lexeme","filter":{},
                "page_size":10,"top_k":10}),
        )
        .await
        .unwrap_or_else(|_| panic!("local Task hybrid query during partial coverage"));
        assert!(
            hybrid["result"]["Published"]["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|hit| hit["task"]["id"] == second_id),
            "hybrid retains the uncovered lexical Task"
        );

        resources
            .index_tasks(project, &selected, &cancel)
            .await
            .unwrap();
        let (generation, documents) = pending().await;
        assert!(documents.is_empty());
        resources
            .index_tasks(project, &selected, &cancel)
            .await
            .unwrap();
        let (repeated, documents) = pending().await;
        assert!(documents.is_empty());
        assert_eq!(
            serde_json::to_value(repeated).unwrap(),
            serde_json::to_value(generation).unwrap(),
            "covered indexing performs no new publication"
        );
    })
    .catch_unwind()
    .await;
    // Shutdown precedes propagation so panic cannot orphan the real model child.
    let jobs_stopped = jobs.shutdown().await;
    let foreground_stopped = foreground.shutdown().await;
    if let Err(panic) = exercise {
        std::panic::resume_unwind(panic);
    }
    jobs_stopped.unwrap();
    foreground_stopped.unwrap();
}
