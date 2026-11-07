//! Real Core/JQ handoff fixtures; the small phase body is not the Hub codec.
use graphit_core::{
    artifact_store::{
        ArtifactKey, ArtifactLimits, ArtifactStore, CapturedPublicationScope, CopyLimits,
        NativeDocumentOperationId, NativeDocumentOutcome, NativeDocumentPhase,
        NativeDocumentRecovery, NativeDocumentRecoveryRequest, ScopeLimits,
    },
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    installation_identity::InstallationId,
    native_document_file::{
        CapturedDocumentDirectory, NativeDocumentFileLimits, NativeDocumentJournalWriteBounds,
        NativeDocumentNamespace,
    },
    project_paths::InstallationPaths,
};
use serde_json::value::RawValue;
use std::{
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

fn store(root: &Path) -> ArtifactStore {
    let installation =
        InstallationPaths::resolve(Brand::default(), root, None, Some(&root.join("owner")))
            .unwrap();
    let scope = CapturedPublicationScope::installation(
        &installation,
        InstallationId::new("terminal-fixture").unwrap(),
        [root.to_owned()],
        ScopeLimits::new(4, 65536, 65536).unwrap(),
        &native_capture_request(),
    )
    .unwrap();
    let copy = CopyLimits::new(1024 * 1024, 1024, 65536, 64).unwrap();
    ArtifactStore::new(scope, ArtifactLimits::new(1024 * 1024, 1024, copy).unwrap())
}
fn context(cancel: Cancellation) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancel,
    ))
}
fn limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(1024 * 1024, 65536, 256, 65536).unwrap()
}
fn raw(text: &str) -> &RawValue {
    serde_json::from_str(text).unwrap()
}
fn write_bounds(text: &str, previous: usize) -> NativeDocumentJournalWriteBounds {
    let desired = NativeDocumentNamespace::admit_journal_body(raw(text), 4096).unwrap();
    NativeDocumentJournalWriteBounds::new(4096, previous, desired).unwrap()
}
fn body(id: NativeDocumentOperationId, phase: &str) -> String {
    format!("{{\"operation_id\":\"{id}\",\"phase\":\"{phase}\"}}")
}
fn phase(store: &ArtifactStore) -> Option<NativeDocumentPhase> {
    match store.load().unwrap()?.pending()? {
        graphit_core::artifact_store::ArtifactIntent::NativeDocument(intent) => {
            Some(intent.phase())
        }
        _ => panic!("unexpected operation"),
    }
}

#[test]
fn cleaned_terminal_survives_drop_and_fresh_recovery_until_real_journal_release() {
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    fs::create_dir(root.join("owner")).unwrap();
    fs::write(root.join("agent.json"), b"{}").unwrap();
    let s = store(&root);
    let ctx = context(Cancellation::default());
    let id = NativeDocumentOperationId::generate();
    let starting = body(id, "starting");
    let projected = body(id, "projected");
    let released = body(id, "released");

    let starting_write = write_bounds(&starting, 0);
    let projected_write = write_bounds(&projected, 4096);
    let _released_write = write_bounds(&released, 4096);

    let ns = NativeDocumentNamespace::capture(s.scope(), &root, limits(), &ctx).unwrap();
    let guard = ns.lock(&ctx).unwrap();
    let first = guard
        .replace_journal(
            &guard.read_journal(4096).unwrap(),
            raw(&starting),
            starting_write,
        )
        .unwrap();
    let parent = CapturedDocumentDirectory::capture(&root, limits(), &ctx).unwrap();
    let old = parent.read("agent.json".as_ref(), &ctx).unwrap();
    let previous = old.receipt().cloned();
    let identity = parent.directory_identity();
    let scope = s.scope().binding().unwrap();
    let key = ArtifactKey::new("kiro", "agent", "terminal-fixture").unwrap();
    let mut writer = s
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .with_io_context(ctx.clone())
        .unwrap();
    let stage = writer
        .prepare_native_document(id, &key, &parent, &old, b"{\"owned\":true}")
        .unwrap_or_else(|f| panic!("{:?}", f.error()));
    #[cfg(windows)]
    let prepared = serde_json::to_value(stage.receipt()).unwrap();
    let resolution = stage.publish().unwrap_or_else(|f| {
        #[cfg(windows)]
        {
            // Report only mismatch categories, never IDs, SIDs, hashes or bodies.
            let observed = parent
                .read("agent.json".as_ref(), &ctx)
                .unwrap_or_else(|e| {
                    panic!("{:?}; destination observation: {:?}", f.error(), e.kind())
                });
            let observed = serde_json::to_value(
                observed
                    .receipt()
                    .unwrap_or_else(|| panic!("{:?}; destination absent", f.error())),
            )
            .unwrap();
            let previous = serde_json::to_value(previous.as_ref().unwrap()).unwrap();
            let stage_absent = matches!(
                fs::symlink_metadata(root.join(format!(".graphit-doc-{id}")).join("document")),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound
            );
            panic!(
                "{:?}; stage_absent={stage_absent}, stage_volume={}, stage_index={}, previous_creation={}, stage_length={}, stage_content={}, previous_access={}, stage_access={}",
                f.error(),
                observed["file_id"]["volume"] == prepared["file_id"]["volume"],
                observed["file_id"]["index"] == prepared["file_id"]["index"],
                observed["file_id"]["created"] == previous["file_id"]["created"],
                observed["length"] == prepared["length"],
                observed["content"] == prepared["content"],
                observed["access"] == previous["access"],
                observed["access"] == prepared["access"],
            );
        }
        #[cfg(not(windows))]
        panic!("{:?}", f.error());
    });
    assert_eq!(resolution.outcome(), NativeDocumentOutcome::Applied);
    let actual = resolution.destination_receipt().unwrap().clone();
    let second = guard
        .replace_journal(&first, raw(&projected), projected_write)
        .unwrap();
    let completion = resolution.acknowledge().unwrap();
    assert!(completion.destination_receipt() == Some(&actual));
    assert!(!root.join(format!(".graphit-doc-{id}")).exists());
    drop(completion);
    // Recovery reserves its own future write; the unused original reservation is retained.

    assert_eq!(phase(&s), Some(NativeDocumentPhase::Cleaned));
    drop(writer);
    drop(guard);
    drop(ns);
    drop(second);

    let fresh = store(&root);
    let ctx = context(Cancellation::default());

    let released_write = write_bounds(&released, 4096);
    let ns = NativeDocumentNamespace::capture(fresh.scope(), &root, limits(), &ctx).unwrap();
    let guard = ns.lock(&ctx).unwrap();
    let snapshot = guard.read_journal(4096).unwrap();
    assert_eq!(snapshot.body().unwrap().unwrap().get(), projected);
    let parent = CapturedDocumentDirectory::capture(&root, limits(), &ctx).unwrap();
    let mut writer = fresh
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .with_io_context(ctx.clone())
        .unwrap();
    let recovered = writer
        .recover_native_document(
            NativeDocumentRecoveryRequest {
                operation_id: id,
                key: &key,
                destination: &root.join("agent.json"),
                parent_identity: &identity,
                previous: previous.as_ref(),
                scope: &scope,
            },
            &parent,
        )
        .unwrap();
    let NativeDocumentRecovery::Completion(completion) = recovered else {
        panic!("terminal required")
    };
    assert_eq!(completion.outcome(), NativeDocumentOutcome::Applied);
    assert!(completion.destination_receipt() == Some(&actual));
    // A historical cleanup does not acquire ownership of subsequent user edits.
    fs::write(root.join("agent.json"), b"{\"user\":true}").unwrap();
    let final_snapshot = guard
        .replace_journal(&snapshot, raw(&released), released_write)
        .unwrap();
    assert_eq!(final_snapshot.body().unwrap().unwrap().get(), released);
    completion.release().unwrap();
    assert_eq!(phase(&fresh), None);
    assert_eq!(
        fs::read(root.join("agent.json")).unwrap(),
        b"{\"user\":true}"
    );
}

#[test]
fn cancelled_caller_released_save_keeps_core_terminal_and_previous_journal() {
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    fs::create_dir(root.join("owner")).unwrap();
    let s = store(&root);
    let cancel = Cancellation::default();
    let ctx = context(cancel.clone());
    let id = NativeDocumentOperationId::generate();
    let projected = body(id, "projected");
    let released = body(id, "released");

    let projected_write = write_bounds(&projected, 0);
    let released_write = write_bounds(&released, 4096);
    let ns = NativeDocumentNamespace::capture(s.scope(), &root, limits(), &ctx).unwrap();
    let guard = ns.lock(&ctx).unwrap();
    let snapshot = guard
        .replace_journal(
            &guard.read_journal(4096).unwrap(),
            raw(&projected),
            projected_write,
        )
        .unwrap();
    let parent = CapturedDocumentDirectory::capture(&root, limits(), &ctx).unwrap();
    let old = parent.read("agent.json".as_ref(), &ctx).unwrap();
    let key = ArtifactKey::new("kiro", "agent", "cancel-fixture").unwrap();
    let mut writer = s
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .with_io_context(ctx.clone())
        .unwrap();
    let completion = writer
        .prepare_native_document(id, &key, &parent, &old, b"{\"owned\":true}")
        .unwrap_or_else(|f| panic!("{:?}", f.error()))
        .publish()
        .unwrap_or_else(|f| panic!("{:?}", f.error()))
        .acknowledge()
        .unwrap();
    cancel.cancel();
    assert!(
        guard
            .replace_journal(&snapshot, raw(&released), released_write)
            .is_err()
    );
    drop(completion);
    drop(writer);
    drop(guard);
    drop(ns);
    let fresh = store(&root);
    assert_eq!(phase(&fresh), Some(NativeDocumentPhase::Cleaned));
    let ctx = context(Cancellation::default());
    let ns = NativeDocumentNamespace::capture(fresh.scope(), &root, limits(), &ctx).unwrap();
    let guard = ns.lock(&ctx).unwrap();
    assert_eq!(
        guard
            .read_journal(4096)
            .unwrap()
            .body()
            .unwrap()
            .unwrap()
            .get(),
        projected
    );
    assert_eq!(
        fs::read(root.join("agent.json")).unwrap(),
        b"{\"owned\":true}"
    );
}

fn native_capture_request() -> std::sync::Arc<graphit_core::file_io::SnapshotContext> {
    std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        graphit_core::file_lock::Cancellation::default(),
    ))
}
