//! Test-only initial publication through native Knowledge Auth and Lance.
use super::*;
use graphit_auth::{ProjectWriteOperation, ScopeInput};
use graphit_data::{
    knowledge_document_projection::{self, AuthoredPageFields, KnowledgeDocumentProjectionLimits},
    knowledge_index::{self, KnowledgeFirstPublication, KnowledgeIndexWriteLimits},
};
use std::num::NonZeroUsize;

pub(crate) async fn seed(
    server: &TaskServer,
    raw: &str,
    selected: usize,
    pages: &[AuthoredPageFields<'_>],
) -> KnowledgeFirstPublication {
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let (paths, id, version) = &snapshot.projects[selected];
    let project =
        CapturedPublicationScope::project(paths, *id, server.0.policy.scope, original.snapshot())
            .unwrap();
    let binding = project.binding().unwrap();
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority
        .admit_authenticated_write(
            &caller,
            &original,
            ScopeInput {
                installation: server.0.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &project,
                        record: &binding,
                    },
                    version,
                }),
                providers: &[],
                binding: server.0.policy.binding,
                global_version: "",
            },
            ProjectWriteOperation::Knowledge,
        )
        .unwrap();
    let c = server.0.policy.content;
    let nz = |n| NonZeroUsize::new(n).unwrap();
    let rows = knowledge_document_projection::prepare(
        original.snapshot(),
        pages,
        &[],
        KnowledgeDocumentProjectionLimits {
            max_documents: nz(c.max_nodes),
            max_observations: nz(c.max_nodes),
            max_rows: nz(c.max_nodes),
            max_field_bytes: nz(c.max_input_bytes),
            max_text_bytes: nz(c.max_input_bytes),
        },
    )
    .unwrap();
    knowledge_index::publish_native_authored(
        original.snapshot(),
        &admission,
        &rows,
        KnowledgeIndexWriteLimits {
            content: crate::foreground_knowledge::limits(&server.0.policy)
                .unwrap()
                .content,
            native: server.0.policy.read.native,
            artifacts: server.0.policy.write.artifacts,
        },
    )
    .await
    .unwrap()
}

pub(crate) fn corpus_path(server: &TaskServer) -> std::path::PathBuf {
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(10),
    )
    .unwrap();
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let (paths, id, _) = &snapshot.projects[0];
    CapturedPublicationScope::project(paths, *id, server.0.policy.scope, request.snapshot())
        .unwrap()
        .runtime_root()
        .join("knowledge/index.lance")
}
pub(crate) fn corrupt_manifest(server: &TaskServer) {
    let mut count = 0;
    for file in std::fs::read_dir(corpus_path(server).join("_versions")).unwrap() {
        let path = file.unwrap().path();
        if path.extension().is_some_and(|s| s == "manifest") {
            std::fs::write(path, b"invalid actual SDK manifest").unwrap();
            count += 1;
        }
    }
    assert!(count > 0);
}

pub(crate) async fn corrupt_unselected_source_path(server: &TaskServer) {
    use arrow_array::{RecordBatch, StringArray};
    use tokio_stream::StreamExt;
    let path = corpus_path(server);
    let store = graphit_data::lance_module_store::LanceModuleStore::open(
        path.to_str().unwrap(),
        knowledge_index::physical_schema(),
    )
    .await
    .unwrap();
    let snapshot = store.snapshot();
    let mut stream = snapshot.scan().try_into_stream().await.unwrap();
    let mut batches = Vec::new();
    while let Some(batch) = stream.next().await {
        let batch = batch.unwrap();
        let mut columns = batch.columns().to_vec();
        let slugs = batch
            .column_by_name("slug")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let old = batch
            .column_by_name("source_path")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let replaced = StringArray::from_iter_values((0..batch.num_rows()).map(|i| {
            if slugs.value(i) == "z-tail" {
                "../outside.md"
            } else {
                old.value(i)
            }
        }));
        let index = batch.schema().index_of("source_path").unwrap();
        columns[index] = Arc::new(replaced);
        batches.push(RecordBatch::try_new(batch.schema(), columns).unwrap());
    }
    drop(stream);
    snapshot
        .prepare_replace(batches)
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
}

pub(crate) fn swap_project_root(server: &TaskServer) {
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let root = snapshot.projects[0].0.root();
    let identity = snapshot.projects[0].0.identity_file();
    let foreign = std::fs::read(snapshot.projects[1].0.identity_file()).unwrap();
    std::fs::rename(root, root.with_extension("original")).unwrap();
    std::fs::create_dir_all(identity.parent().unwrap()).unwrap();
    std::fs::write(identity, foreign).unwrap();
}
