//! Behavioural installation tests; directory capture, physical receipts and recovery phases are obsolete.
use super::*;
use crate::{
    brand::Brand,
    project_paths::{InstallationPaths, ProjectPaths},
};
use std::sync::Barrier;
const ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
struct Fixture {
    _temp: tempfile::TempDir,
    store: ArtifactStore,
    source: PathBuf,
    destination: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let installation =
            InstallationPaths::resolve(Brand::default(), root, None, Some(&root.join("global")))
                .unwrap();
        let paths = ProjectPaths::at(root, &installation).unwrap();
        let store = ArtifactStore::new(
            PublicationScope::project(&paths, ID.parse().unwrap()).unwrap(),
            ArtifactLimits::new(
                1024 * 1024,
                1024,
                CopyLimits::new(1024 * 1024, 100).unwrap(),
            )
            .unwrap(),
        );
        let source = root.join("source ação");
        fs::write(&source, b"first").unwrap();
        let destination = paths.root().join("adapter/skill");
        Self {
            _temp: temp,
            store,
            source,
            destination,
        }
    }
    fn writer(&self) -> ArtifactWriter {
        self.store
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
    }
    fn key(&self) -> ArtifactKey {
        ArtifactKey::new("test", "skill", "guide").unwrap()
    }
    fn publish(&self, writer: &mut ArtifactWriter) -> ArtifactResult<ArtifactChange> {
        writer.publish(
            &self.key(),
            &self.source,
            &self.destination,
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::FollowLinks),
        )
    }
}
#[test]
fn copy_create_noop_update_reopen_remove_preserves_source() {
    let f = Fixture::new();
    assert!(f.store.load().unwrap().is_none());
    let mut writer = f.writer();
    assert_eq!(f.publish(&mut writer).unwrap(), ArtifactChange::Created);
    assert_eq!(fs::read(&f.destination).unwrap(), b"first");
    assert_eq!(f.publish(&mut writer).unwrap(), ArtifactChange::Unchanged);
    fs::write(&f.source, b"second").unwrap();
    assert_eq!(f.publish(&mut writer).unwrap(), ArtifactChange::Updated);
    assert_eq!(fs::read(&f.destination).unwrap(), b"second");
    drop(writer);
    let mut writer = f.writer();
    assert_eq!(writer.remove(&f.key()).unwrap(), ArtifactChange::Removed);
    assert!(!f.destination.exists());
    assert_eq!(fs::read(&f.source).unwrap(), b"second");
    let text = fs::read_to_string(f.store.manifest_path()).unwrap();
    for removed in ["pending", "physical", "inode", "receipt", "backup", "scope"] {
        assert!(!text.contains(removed), "{removed}");
    }
}
#[test]
fn generated_nested_paths_match_capture_noop_and_preserve_user_edits() {
    let f = Fixture::new();
    let mut writer = f.writer();
    let policy = PayloadPolicy::new(
        NodeKind::Directory,
        PayloadMode::private_directory(),
        CopyLimits::new(1024, 10).unwrap(),
    );
    let generated = |path: &str| {
        ValidatedPayloadPlan::generated(
            [
                GeneratedNode::directory(PathBuf::new(), PayloadMode::private_directory()).unwrap(),
                GeneratedNode::directory("references".into(), PayloadMode::private_directory())
                    .unwrap(),
                GeneratedNode::file(
                    path.into(),
                    b"execution guide".to_vec(),
                    PayloadMode::private_file(),
                )
                .unwrap(),
            ],
            &policy,
            &writer.context,
        )
        .unwrap()
    };
    let plan = generated("references/execution.md");
    let equivalent = generated("references//execution.md");
    assert_eq!(plan.fingerprint(), equivalent.fingerprint());
    let mode = ArtifactMode::Copy(CopyPolicy::FollowLinks);
    assert_eq!(
        writer
            .publish_generated(&f.key(), &plan, &f.destination, NodeKind::Directory, mode)
            .unwrap(),
        ArtifactChange::Created
    );
    let captured =
        SourceSnapshot::capture_generated(&f.destination, NodeKind::Directory, &writer.context)
            .unwrap();
    assert_eq!(plan.fingerprint(), captured.fingerprint());
    assert_eq!(
        writer
            .check_generated_publication(&f.key(), &plan, &f.destination, NodeKind::Directory, mode)
            .unwrap(),
        PublicationCheck::VerifiedUnchanged
    );
    assert_eq!(
        writer
            .publish_generated(
                &f.key(),
                &equivalent,
                &f.destination,
                NodeKind::Directory,
                mode
            )
            .unwrap(),
        ArtifactChange::Unchanged
    );
    let edited = f.destination.join("references").join("execution.md");
    fs::write(&edited, b"user content").unwrap();
    assert_eq!(
        writer
            .publish_generated(&f.key(), &plan, &f.destination, NodeKind::Directory, mode)
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(edited).unwrap(), b"user content");
}

#[test]
fn user_edit_blocks_update_and_remove_but_forget_only_changes_metadata() {
    let f = Fixture::new();
    let mut writer = f.writer();
    f.publish(&mut writer).unwrap();
    fs::write(&f.destination, b"user content").unwrap();
    fs::write(&f.source, b"upgrade").unwrap();
    assert_eq!(
        f.publish(&mut writer).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        writer.remove(&f.key()).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    writer.forget(&f.key()).unwrap();
    assert_eq!(fs::read(&f.destination).unwrap(), b"user content");
    assert!(f.store.load().unwrap().unwrap().entries().is_empty());
}
#[test]
fn foreign_file_is_not_overwritten_or_adopted() {
    let f = Fixture::new();
    fs::create_dir_all(f.destination.parent().unwrap()).unwrap();
    fs::write(&f.destination, b"foreign").unwrap();
    assert_eq!(
        f.publish(&mut f.writer()).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&f.destination).unwrap(), b"foreign");
    assert!(f.store.load().unwrap().is_none());
}
#[test]
fn independent_writers_do_not_lose_other_entries() {
    let f = Fixture::new();
    let barrier = Arc::new(Barrier::new(3));
    let mut threads = Vec::new();
    for index in 0..2 {
        let store = f.store.clone();
        let source = f.source.clone();
        let destination = f.destination.with_extension(format!("{index}"));
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let mut writer = store
                .writer(Duration::from_secs(5), &Cancellation::default())
                .unwrap();
            barrier.wait();
            writer
                .publish(
                    &ArtifactKey::new("test", "skill", &index.to_string()).unwrap(),
                    &source,
                    &destination,
                    NodeKind::File,
                    ArtifactMode::Copy(CopyPolicy::FollowLinks),
                )
                .unwrap();
        }));
    }
    barrier.wait();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(f.store.load().unwrap().unwrap().entries().len(), 2);
}
#[test]
fn cancellation_before_publication_leaves_destination_untouched() {
    let f = Fixture::new();
    let cancellation = Cancellation::default();
    let mut writer = f
        .store
        .writer(Duration::from_secs(5), &cancellation)
        .unwrap();
    cancellation.cancel();
    assert_eq!(
        f.publish(&mut writer).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!f.destination.exists());
    assert!(f.store.load().unwrap().is_none());
}
#[test]
fn malformed_catalogue_is_reported_without_modifying_files() {
    let f = Fixture::new();
    fs::create_dir_all(f.store.runtime_root()).unwrap();
    fs::write(f.store.manifest_path(), b"{broken").unwrap();
    assert_eq!(
        f.store.load().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), b"{broken");
}
#[test]
fn missing_owned_output_is_recreated_by_explicit_publication() {
    let f = Fixture::new();
    let mut writer = f.writer();
    f.publish(&mut writer).unwrap();
    fs::remove_file(&f.destination).unwrap();
    assert_eq!(f.publish(&mut writer).unwrap(), ArtifactChange::Updated);
    assert_eq!(fs::read(&f.destination).unwrap(), b"first");
}
#[test]
fn failed_metadata_write_reports_applied_output_and_explicit_retry_finishes_metadata() {
    let f = Fixture::new();
    let mut writer = f.writer();
    f.publish(&mut writer).unwrap();
    fs::write(&f.source, b"upgrade").unwrap();
    let original_limit = writer.store.limits.max_document_bytes;
    writer.store.limits.max_document_bytes =
        fs::metadata(f.store.manifest_path()).unwrap().len() as usize; // Set exact existing capacity, then a larger key forces catalogue overflow.
    let extra = ArtifactEntry {
        key: ArtifactKey::new("test", "skill", "extra").unwrap(),
        slot: writer.publication_slot(),
        source: f.source.clone(),
        generated_origin: None,
        destination: f.destination.with_extension("other"),
        kind: codec::Kind::File,
        mode: ArtifactMode::Copy(CopyPolicy::FollowLinks),
        content_hash: Some([0; 32]),
    };
    let mut doc = writer.document().unwrap();
    doc.entries.push(extra);
    writer.store.limits.max_document_bytes = original_limit;
    writer.save(&doc, false).unwrap();
    // Force persistence to fail only after the file replacement, without preventing the current catalogue read.
    let current = fs::read(f.store.manifest_path()).unwrap();
    let current_doc: serde_json::Value = serde_json::from_slice(&current).unwrap();
    let compact = serde_json::to_vec(&current_doc).unwrap();
    fs::write(f.store.manifest_path(), &compact).unwrap();
    writer.store.limits.max_document_bytes = compact.len();
    // A longer recorded source name changes the resulting document length.
    let longer = f.source.with_file_name("source with a longer name ação");
    fs::write(&longer, b"upgrade").unwrap();
    let err = writer
        .publish(
            &f.key(),
            &longer,
            &f.destination,
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::FollowLinks),
        )
        .unwrap_err();
    assert_eq!(err.effect(), ArtifactEffect::Applied);
    assert_eq!(fs::read(&f.destination).unwrap(), b"upgrade");
    writer.store.limits.max_document_bytes = original_limit;
    writer
        .publish(
            &f.key(),
            &longer,
            &f.destination,
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::FollowLinks),
        )
        .unwrap();
    assert_eq!(fs::read(&f.destination).unwrap(), b"upgrade");
}
#[cfg(unix)]
#[test]
fn configured_symlink_roots_hardlinks_and_benign_modes_are_supported() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    let real = f._temp.path().join("real");
    fs::create_dir(&real).unwrap();
    fs::create_dir_all(f.destination.parent().unwrap()).unwrap();
    fs::remove_dir(f.destination.parent().unwrap()).unwrap();
    symlink(&real, f.destination.parent().unwrap()).unwrap();
    let alias = f.source.with_extension("alias");
    fs::hard_link(&f.source, &alias).unwrap();
    let mut writer = f.writer();
    writer
        .publish(
            &f.key(),
            &alias,
            &f.destination,
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::FollowLinks),
        )
        .unwrap();
    fs::set_permissions(&f.destination, fs::Permissions::from_mode(0o640)).unwrap();
    assert!(
        writer
            .verify_owned_copy(&f.key(), &f.destination, NodeKind::File)
            .unwrap()
    );
    assert_eq!(fs::read(real.join("skill")).unwrap(), b"first");
}
#[cfg(unix)]
#[test]
fn link_update_remove_never_changes_the_source() {
    let f = Fixture::new();
    let mut writer = f.writer();
    writer
        .publish(
            &f.key(),
            &f.source,
            &f.destination,
            NodeKind::File,
            ArtifactMode::Link,
        )
        .unwrap();
    assert!(
        fs::symlink_metadata(&f.destination)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::canonicalize(&f.destination).unwrap(),
        fs::canonicalize(&f.source).unwrap()
    );
    assert_eq!(fs::read(&f.destination).unwrap(), b"first");
    fs::write(&f.source, b"updated").unwrap();
    assert_eq!(fs::read(&f.destination).unwrap(), b"updated");
    writer.remove(&f.key()).unwrap();
    assert!(!f.destination.exists());
    assert_eq!(fs::read(&f.source).unwrap(), b"updated");
}

#[test]
fn directory_update_preserves_unrelated_files_and_removes_old_package_entries() {
    let f = Fixture::new();
    fs::remove_file(&f.source).unwrap();
    fs::create_dir(&f.source).unwrap();
    fs::create_dir(f.source.join("empty")).unwrap();
    fs::write(f.source.join("old"), b"old").unwrap();
    fs::write(f.source.join("ação.md"), b"unicode").unwrap();
    let mode = ArtifactMode::Copy(CopyPolicy::FollowLinks);
    let mut writer = f.writer();
    writer
        .publish(
            &f.key(),
            &f.source,
            &f.destination,
            NodeKind::Directory,
            mode,
        )
        .unwrap();
    let unrelated = f.destination.parent().unwrap().join("user-file");
    fs::write(&unrelated, b"keep").unwrap();
    fs::remove_file(f.source.join("old")).unwrap();
    fs::write(f.source.join("new"), b"new").unwrap();
    writer
        .publish(
            &f.key(),
            &f.source,
            &f.destination,
            NodeKind::Directory,
            mode,
        )
        .unwrap();
    assert!(!f.destination.join("old").exists());
    assert_eq!(fs::read(f.destination.join("new")).unwrap(), b"new");
    assert_eq!(fs::read(f.destination.join("ação.md")).unwrap(), b"unicode");
    assert!(f.destination.join("empty").is_dir());
    assert_eq!(fs::read(unrelated).unwrap(), b"keep");
}
#[cfg(unix)]
#[test]
fn directory_links_follow_configured_policy_and_untrusted_entry_policy_is_explicit() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    fs::remove_file(&f.source).unwrap();
    fs::create_dir(&f.source).unwrap();
    let real = f._temp.path().join("source-dir");
    fs::create_dir(&real).unwrap();
    fs::write(real.join("doc"), b"linked").unwrap();
    symlink(&real, f.source.join("nested")).unwrap();
    let mut writer = f.writer();
    writer
        .publish(
            &f.key(),
            &f.source,
            &f.destination,
            NodeKind::Directory,
            ArtifactMode::Copy(CopyPolicy::FollowLinks),
        )
        .unwrap();
    assert_eq!(
        fs::read(f.destination.join("nested/doc")).unwrap(),
        b"linked"
    );
    let foreign = f.destination.with_extension("untrusted");
    assert!(
        writer
            .publish(
                &ArtifactKey::new("test", "skill", "untrusted").unwrap(),
                &f.source,
                &foreign,
                NodeKind::Directory,
                ArtifactMode::Copy(CopyPolicy::RejectEntryLinks)
            )
            .is_err()
    );
    assert!(!foreign.exists());
    assert_eq!(fs::read(real.join("doc")).unwrap(), b"linked");
}
#[cfg(target_os = "linux")]
#[test]
#[ignore = "native qualification needs a second writable filesystem"]
fn publication_reads_a_source_on_another_filesystem() {
    use std::os::unix::fs::MetadataExt;
    let mut f = Fixture::new();
    let source = tempfile::tempdir_in("/dev/shm").unwrap();
    assert_ne!(
        fs::metadata(f._temp.path()).unwrap().dev(),
        fs::metadata(source.path()).unwrap().dev(),
        "qualification requires distinct filesystems"
    );
    f.source = source.path().join("remote-volume.txt");
    fs::write(&f.source, b"cross-volume").unwrap();
    f.publish(&mut f.writer()).unwrap();
    assert_eq!(fs::read(f.destination).unwrap(), b"cross-volume");
    assert_eq!(fs::read(f.source).unwrap(), b"cross-volume");
}
