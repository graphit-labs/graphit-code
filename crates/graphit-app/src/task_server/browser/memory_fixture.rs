//! Test-only writes through actual native Auth/PROJECT admission and Lance publication.
use super::*;
use graphit_auth::{ProjectWriteOperation, ScopeInput};
use graphit_records::memory_store::{
    CreateMemory, DeleteMemory, Memory, MemoryStoreWriteLimits, NativeMemoryStore, UpdateMemory,
};

pub(crate) enum Seed {
    Create(CreateMemory),
    Update(graphit_records::memory_store::MemoryId, UpdateMemory),
    Delete(graphit_records::memory_store::MemoryId, DeleteMemory),
}
pub(crate) async fn seed(
    server: &TaskServer,
    raw: &str,
    selected: usize,
    operation: Seed,
) -> Memory {
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
            ProjectWriteOperation::Memory,
        )
        .unwrap();
    let limits = MemoryStoreWriteLimits {
        read: crate::foreground_memory::limits(&server.0.policy).unwrap(),
        artifacts: server.0.policy.write.artifacts,
        max_input_bytes: std::num::NonZeroUsize::new(server.0.policy.content.max_input_bytes)
            .unwrap(),
        max_row_bytes: std::num::NonZeroUsize::new(server.0.policy.content.max_output_bytes)
            .unwrap(),
    };
    match operation {
        Seed::Create(input) => {
            NativeMemoryStore::create(original.snapshot(), &admission, input, limits)
                .await
                .unwrap()
        }
        Seed::Update(id, input) => {
            NativeMemoryStore::update(original.snapshot(), &admission, id, input, limits)
                .await
                .unwrap()
        }
        Seed::Delete(id, input) => {
            NativeMemoryStore::delete(original.snapshot(), &admission, id, input, limits)
                .await
                .unwrap()
        }
    }
}
pub(crate) fn selection(server: &TaskServer, selected: usize) -> Value {
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let (paths, id, version) = &snapshot.projects[selected];
    json!({"project_dir":graphit_core::native_path::NativePath::new(paths.root().to_owned()).unwrap(),"project_id":id,"project_version":version.version()})
}

fn manifest_files(server: &TaskServer) -> Vec<std::path::PathBuf> {
    let cancellation = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancellation,
        Instant::now() + Duration::from_secs(10),
    )
    .unwrap();
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let (paths, id, _) = &snapshot.projects[0];
    let scope =
        CapturedPublicationScope::project(paths, *id, server.0.policy.scope, original.snapshot())
            .unwrap();
    let versions = scope.runtime_root().join("memory/index.lance/_versions");
    let mut files = std::fs::read_dir(versions)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "manifest"))
        .collect::<Vec<_>>();
    assert!(!files.is_empty());
    files.sort();
    files
}
pub(crate) fn corrupt_manifest(server: &TaskServer) {
    for file in manifest_files(server) {
        let mut data = std::fs::read(&file).unwrap();
        let offset =
            u64::from_le_bytes(data[data.len() - 16..data.len() - 8].try_into().unwrap()) as usize;
        assert!(offset + 4 < data.len() - 16);
        data[offset + 4] = 0; // Protobuf tag zero is invalid; footer and length remain actual SDK bytes.
        std::fs::write(file, data).unwrap();
    }
}

pub(crate) fn with_named_version(server: &TaskServer, name: &str) -> TaskServer {
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let mut projects = snapshot.projects.as_ref().clone();
    let (paths, id, _) = &projects[0];
    let cancellation = Cancellation::default();
    let checkout = graphit_core::git_checkout::GitCheckout::inspect(
        paths.root(),
        Duration::from_secs(5),
        &cancellation,
    )
    .unwrap();
    let version = graphit_core::version_context::VersionContext::select(
        paths,
        checkout.as_checkout(),
        name,
        None,
        "",
    )
    .unwrap();
    projects.push((paths.clone(), *id, version));
    TaskServer::new(
        Arc::clone(&server.0.auth),
        Arc::new(projects),
        snapshot.current.expect("active fixture"),
        server.0.policy.clone(),
        CancellationToken::new(),
    )
    .unwrap()
}

pub(crate) fn corrupt_short_manifest(server: &TaskServer) {
    for file in manifest_files(server) {
        std::fs::write(file, b"invalid actual SDK manifest").unwrap();
    }
}
