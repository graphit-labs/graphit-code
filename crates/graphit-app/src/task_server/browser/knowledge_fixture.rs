//! Test-only initial publication through native Knowledge Auth and Lance.
use super::*;
use arrow_array::{Array, ArrayRef, RecordBatch, StringArray, UInt8Array};
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
    super::memory_fixture::corrupt_lance_manifests(&corpus_path(server).join("_versions"));
}

pub(crate) async fn corrupt_unselected_source_path(server: &TaskServer) {
    corrupt_tail_column(server, "source_path", |slugs, column| {
        let old = column.as_any().downcast_ref::<StringArray>().unwrap();
        Arc::new(StringArray::from_iter_values((0..slugs.len()).map(|i| {
            if slugs.value(i) == "z-tail" {
                "../outside.md"
            } else {
                old.value(i)
            }
        })))
    })
    .await;
}

/// Both the page and heads readers validate this projected state, even without
/// a reference projection. Return the actual commit pin for deep-clone fixtures.
pub(crate) async fn corrupt_tail_document_state(server: &TaskServer) -> u64 {
    corrupt_tail_column(server, "document_state", |slugs, column| {
        let old = column.as_any().downcast_ref::<UInt8Array>().unwrap();
        Arc::new(UInt8Array::from_iter_values((0..slugs.len()).map(|i| {
            if slugs.value(i) == "z-tail" {
                0
            } else {
                old.value(i)
            }
        })))
    })
    .await
}

async fn corrupt_tail_column(
    server: &TaskServer,
    name: &str,
    replace: impl Fn(&StringArray, &ArrayRef) -> ArrayRef,
) -> u64 {
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
    let mut tails = 0;
    while let Some(batch) = stream.next().await {
        let batch = batch.unwrap();
        let mut columns = batch.columns().to_vec();
        let slugs = batch
            .column_by_name("slug")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        tails += slugs.iter().filter(|slug| *slug == Some("z-tail")).count();
        let index = batch.schema().index_of(name).unwrap();
        columns[index] = replace(slugs, &columns[index]);
        batches.push(RecordBatch::try_new(batch.schema(), columns).unwrap());
    }
    drop(stream);
    assert_eq!(tails, 1);
    snapshot
        .prepare_replace(batches)
        .await
        .unwrap()
        .commit()
        .await
        .unwrap()
        .snapshot()
        .version()
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
