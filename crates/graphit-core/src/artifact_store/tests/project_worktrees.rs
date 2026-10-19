use super::*;

struct Worktrees {
    a: ArtifactStore,
    b: ArtifactStore,
    source: PathBuf,
    _directory: tempfile::TempDir,
}
impl Worktrees {
    fn new() -> Self {
        Self::with_checkouts(["a", "b"])
    }
    fn with_checkouts(names: [&str; 2]) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        let global = root.join("global");
        fs::create_dir(&global).unwrap();
        let installation = crate::project_paths::InstallationPaths::resolve(
            crate::brand::Brand::default(),
            &root,
            None,
            Some(&global),
        )
        .unwrap();
        let stores = names.map(|name| {
            let checkout = root.join(name);
            fs::create_dir(&checkout).unwrap();
            let project = ProjectPaths::at(&checkout, &installation).unwrap();
            ArtifactStore::new(
                fixture_scope(&project, &installation, false),
                ArtifactLimits::new(
                    1024 * 1024,
                    1024,
                    CopyLimits::new(1024 * 1024, 128, 16 * 1024, 8).unwrap(),
                )
                .unwrap(),
            )
        });
        let [a, b] = stores;
        let source = root.join("source");
        fs::write(&source, b"shared source").unwrap();
        Self {
            a,
            b,
            source,
            _directory: directory,
        }
    }
    fn destination(store: &ArtifactStore) -> PathBuf {
        store.scope.owner_root().join("published")
    }
    fn writer(store: &ArtifactStore) -> ArtifactWriter {
        store
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
    }
    fn publish(
        &self,
        writer: &mut ArtifactWriter,
        key: &ArtifactKey,
        destination: &Path,
        mode: ArtifactMode,
    ) -> ArtifactResult<ArtifactChange> {
        writer.publish(
            key,
            PhysicalSource::ExternalSource(&self.source),
            destination,
            NodeKind::File,
            mode,
        )
    }
}

#[test]
fn catalogue_identity_keeps_project_storage_and_physical_ancestor_isolation() {
    let f = Worktrees::new();
    let root = f.source.parent().unwrap();
    let global = root.join("global");
    let other_global = root.join("other-global");
    fs::create_dir(&other_global).unwrap();
    let capture = |storage: &Path, id: ProjectId| {
        let installation = crate::project_paths::InstallationPaths::resolve(
            crate::brand::Brand::default(),
            root,
            None,
            Some(storage),
        )
        .unwrap();
        let project = ProjectPaths::at(f.b.scope.owner_root(), &installation).unwrap();
        CapturedPublicationScope::project(
            &project,
            id,
            ScopeLimits::new(1, 16 * 1024, 1024).unwrap(),
            &native_capture_request(),
        )
        .unwrap()
    };
    let binding = f.a.scope.binding().unwrap();
    assert!(!binding.matches_catalog(&capture(
        &global,
        "01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap()
    )));
    assert!(!binding.matches_catalog(&capture(&other_global, ID.parse().unwrap())));
    // Preserve the original inode so replacement cannot depend on inode reuse.
    fs::rename(&global, root.join("held-global")).unwrap();
    fs::create_dir(&global).unwrap();
    assert!(!binding.matches_catalog(&capture(&global, ID.parse().unwrap())));
    assert!(!f.a.manifest_path().exists());
}

#[test]
fn publication_slots_allow_same_key_coexistence_and_isolate_update_remove() {
    for mode in [
        ArtifactMode::Link,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
    ] {
        let f = Worktrees::new();
        let key = ArtifactKey::new("adapter", "command", "same-command").unwrap();
        let a = Worktrees::destination(&f.a);
        let b = Worktrees::destination(&f.b);
        assert_eq!(f.a.manifest_path(), f.b.manifest_path());
        assert_ne!(f.a.publication_slot(), f.b.publication_slot());
        let binding = f.a.scope.binding().unwrap();
        assert!(binding.matches_catalog(&f.b.scope));
        assert!(!binding.matches_scope(&f.b.scope));
        {
            let mut writer = Worktrees::writer(&f.a);
            assert_eq!(
                f.publish(&mut writer, &key, &a, mode).unwrap(),
                ArtifactChange::Created
            );
        }
        let mut writer = Worktrees::writer(&f.b);
        assert_eq!(writer.load().unwrap().unwrap().entries().len(), 1);
        assert_eq!(f.b.load().unwrap().unwrap().entries()[0].destination(), a);
        assert_eq!(
            writer.remove(&key).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            f.publish(&mut writer, &key, &a, mode).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(fs::read(&a).unwrap(), b"shared source");
        assert!(!b.exists());
        assert_eq!(
            f.publish(&mut writer, &key, &b, mode).unwrap(),
            ArtifactChange::Created
        );
        drop(writer);
        assert_eq!(f.a.load().unwrap().unwrap().entries().len(), 2);
        let updated = f.source.parent().unwrap().join("updated-source");
        fs::write(&updated, b"updated source").unwrap();
        let mut writer = Worktrees::writer(&f.a);
        assert_eq!(
            writer
                .publish(
                    &key,
                    PhysicalSource::ExternalSource(&updated),
                    &a,
                    NodeKind::File,
                    mode
                )
                .unwrap(),
            ArtifactChange::Updated
        );
        assert_eq!(fs::read(&a).unwrap(), b"updated source");
        assert_eq!(fs::read(&b).unwrap(), b"shared source");
        assert_eq!(writer.remove(&key).unwrap(), ArtifactChange::Removed);
        assert_eq!(
            writer.remove(&key).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            writer.forget(&key).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(writer.load().unwrap().unwrap().entries().len(), 1);
        assert_eq!(
            writer.load().unwrap().unwrap().entries()[0].publication_slot(),
            f.b.publication_slot()
        );
        assert_eq!(fs::read(&b).unwrap(), b"shared source");
        assert_eq!(fs::read(&f.source).unwrap(), b"shared source");
    }
}

#[test]
fn publication_slots_keep_foreign_pending_readable_without_recovery_or_forget() {
    let f = Worktrees::new();
    let key = ArtifactKey::new("adapter", "command", "pending").unwrap();
    let destination = Worktrees::destination(&f.a);
    {
        let mut writer = Worktrees::writer(&f.a);
        writer.fail_at.set(Some(ArtifactStage::Commit));
        assert!(
            f.publish(&mut writer, &key, &destination, ArtifactMode::Link)
                .is_err()
        );
    }
    let original = fs::read(f.a.manifest_path()).unwrap();
    {
        let mut writer = Worktrees::writer(&f.b);
        assert!(writer.load().unwrap().unwrap().pending().is_some());
        assert_eq!(
            writer
                .load()
                .unwrap()
                .unwrap()
                .pending()
                .unwrap()
                .publication_slot(),
            Some(f.a.publication_slot())
        );
        assert_eq!(
            writer.recover().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            writer.forget(&key).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
    assert_eq!(fs::read(f.a.manifest_path()).unwrap(), original);
    assert_eq!(fs::read(&destination).unwrap(), b"shared source");
    let mut writer = Worktrees::writer(&f.a);
    assert_eq!(writer.recover().unwrap(), ArtifactRecovery::CompletedCreate);
    assert_eq!(writer.load().unwrap().unwrap().entries().len(), 1);
}

#[test]
fn publication_slots_use_exact_roots_even_when_checkout_roots_are_nested() {
    let f = Worktrees::with_checkouts(["a", "a/nested"]);
    let key = ArtifactKey::new("adapter", "command", "same").unwrap();
    let a = Worktrees::destination(&f.a);
    let b = Worktrees::destination(&f.b);
    {
        let mut writer = Worktrees::writer(&f.b);
        f.publish(
            &mut writer,
            &key,
            &b,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
        .unwrap();
    }
    let mut writer = Worktrees::writer(&f.a);
    let foreign = writer.load().unwrap().unwrap().entries()[0].clone();
    // The outer root's filesystem scope contains B, but B is not A's publication.
    f.a.destination_scope(&b).unwrap();
    assert_eq!(
        writer.remove(&key).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(
        writer.forget(&key).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    assert!(!writer.verify_owned_copy(&key, &b, NodeKind::File).unwrap());
    assert_eq!(
        writer
            .verify(&foreign, ArtifactStage::Conflict)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        writer.restore_inspect(&foreign).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    f.publish(
        &mut writer,
        &key,
        &a,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
    )
    .unwrap();
    assert_eq!(writer.load().unwrap().unwrap().entries().len(), 2);
    writer.remove(&key).unwrap();
    assert_eq!(fs::read(&b).unwrap(), b"shared source");
}

#[test]
fn publication_slots_require_explicit_valid_data_and_preserve_global_destination_checks() {
    let f = Worktrees::new();
    let key = ArtifactKey::new("adapter", "command", "same").unwrap();
    let mut writer = Worktrees::writer(&f.a);
    f.publish(
        &mut writer,
        &key,
        &Worktrees::destination(&f.a),
        ArtifactMode::Link,
    )
    .unwrap();
    let document = writer.document().unwrap();
    let entry = &document.entries[0];
    let mut value = serde_json::to_value(&entry.candidate).unwrap();
    value.as_object_mut().unwrap().remove("slot");
    assert!(serde_json::from_value::<Candidate>(value).is_err());
    assert!(serde_json::from_str::<PublicationSlot>(r#"{"kind":"scope","extra":true}"#).is_err());
    let mut invalid = document.clone();
    invalid.entries[0].candidate.slot = PublicationSlot::Scope;
    assert!(invalid.validate(&f.a).is_err());
    #[cfg(unix)]
    {
        invalid.entries[0].candidate.slot = PublicationSlot::ProjectRoot(OwnerRootIdentity::Unix {
            device: 0,
            inode: 0,
        });
        assert!(invalid.validate(&f.a).is_err());
    }
    let mut duplicate_destination = document.clone();
    let mut foreign = entry.clone();
    foreign.candidate.slot = f.b.publication_slot();
    duplicate_destination.entries.push(foreign);
    assert!(duplicate_destination.validate(&f.a).is_err());
    assert_eq!(
        serde_json::from_slice::<PublicationSlot>(
            &serde_json::to_vec(&f.a.publication_slot()).unwrap()
        )
        .unwrap(),
        f.a.publication_slot()
    );
}

#[cfg(unix)]
#[test]
fn publication_slots_ignore_permission_changes_on_the_same_physical_root() {
    use std::os::unix::fs::PermissionsExt;
    let f = Worktrees::new();
    let root = f.source.parent().unwrap();
    let installation = crate::project_paths::InstallationPaths::resolve(
        crate::brand::Brand::default(),
        root,
        None,
        Some(&root.join("global")),
    )
    .unwrap();
    fs::set_permissions(f.a.scope.owner_root(), fs::Permissions::from_mode(0o700)).unwrap();
    let project = ProjectPaths::at(f.a.scope.owner_root(), &installation).unwrap();
    let fresh = fixture_scope(&project, &installation, false);
    assert_eq!(fresh.publication_slot(), f.a.publication_slot());
    let binding = fresh
        .project_owner_binding(
            OwnerRootBindingLimits::new(64, 256).unwrap(),
            &native_capture_request(),
        )
        .unwrap();
    assert!(
        matches!(fresh.publication_slot(), PublicationSlot::ProjectRoot(identity) if identity == binding.root_identity())
    );
}

#[test]
fn publication_slots_preserve_installation_scope_publications() {
    let f = Fixture::with_owner(true);
    assert_eq!(f.store.publication_slot(), PublicationSlot::Scope);
    let mut writer = f.writer();
    assert_eq!(writer.publication_slot(), PublicationSlot::Scope);
    writer
        .publish(
            &f.key(),
            PhysicalSource::ExternalSource(&f.source),
            &f.destination,
            NodeKind::File,
            ArtifactMode::Link,
        )
        .unwrap();
    assert_eq!(
        writer.load().unwrap().unwrap().entries()[0].publication_slot(),
        PublicationSlot::Scope
    );
    assert_eq!(writer.remove(&f.key()).unwrap(), ArtifactChange::Removed);
}

#[test]
fn abandoned_checkout_observations_survive_another_worktree_publication() {
    let f = Worktrees::new();
    let key = ArtifactKey::new("adapter", "command", "abandoned").unwrap();
    let destination = Worktrees::destination(&f.a);
    let stage = {
        let mut writer = Worktrees::writer(&f.a);
        writer.fail_at.set(Some(ArtifactStage::Observe));
        assert!(
            f.publish(
                &mut writer,
                &key,
                &destination,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
            )
            .is_err()
        );
        let path = writer
            .load()
            .unwrap()
            .unwrap()
            .pending()
            .unwrap()
            .publication()
            .unwrap()
            .staging_directory()
            .unwrap()
            .to_owned();
        assert_eq!(
            writer.recover().unwrap(),
            ArtifactRecovery::AbortedPublication
        );
        path
    };
    let key_b = ArtifactKey::new("adapter", "command", "b").unwrap();
    let mut writer = Worktrees::writer(&f.b);
    assert_eq!(writer.document().unwrap().abandoned_publications.len(), 1);
    assert_eq!(
        f.publish(
            &mut writer,
            &key_b,
            &Worktrees::destination(&f.b),
            ArtifactMode::Link
        )
        .unwrap(),
        ArtifactChange::Created
    );
    drop(writer);
    let writer = Worktrees::writer(&f.a);
    let document = writer.document().unwrap();
    assert_eq!(document.abandoned_publications.len(), 1);
    assert_eq!(
        document.abandoned_publications[0].operation.stage.path.0,
        stage
    );
    assert!(stage.is_dir());
    assert!(!destination.exists());
    assert_eq!(fs::read(&f.source).unwrap(), b"shared source");
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[test]
fn native_pending_does_not_grant_another_worktree_its_recorded_parent() {
    use crate::native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits};
    let f = Worktrees::new();
    let request = native_capture_request();
    let destination = Worktrees::destination(&f.a);
    let parent = CapturedDocumentDirectory::capture(
        destination.parent().unwrap(),
        NativeDocumentFileLimits::new(1024 * 1024, 65536, 256, 65536).unwrap(),
        &request,
    )
    .unwrap();
    let observation = parent
        .read(destination.file_name().unwrap(), &request)
        .unwrap();
    let key = ArtifactKey::new("adapter", "document", "pending").unwrap();
    let operation_id = NativeDocumentOperationId::generate();
    {
        let mut writer = f.b.writer_with_context(&request).unwrap();
        let failure = writer
            .prepare_native_document(
                operation_id,
                &key,
                &parent,
                &observation,
                b"foreign attempt",
            )
            .err()
            .unwrap();
        assert_eq!(failure.error().kind(), io::ErrorKind::InvalidInput);
        drop(failure);
        assert!(writer.load().unwrap().is_none());
        assert!(!destination.exists());
    }
    {
        let mut writer = f.a.writer_with_context(&request).unwrap();
        let stage = writer
            .prepare_native_document(
                operation_id,
                &key,
                &parent,
                &observation,
                b"native document",
            )
            .unwrap_or_else(|e| panic!("{:?}", e.error()));
        drop(stage);
    }
    let original = fs::read(f.a.manifest_path()).unwrap();
    let mut writer = f.b.writer_with_context(&request).unwrap();
    assert!(writer.load().unwrap().unwrap().pending().is_some());
    let binding = f.b.scope.binding().unwrap();
    let parent_identity = parent.directory_identity();
    let error = writer
        .recover_native_document(
            NativeDocumentRecoveryRequest {
                operation_id,
                key: &key,
                destination: &destination,
                parent_identity: &parent_identity,
                previous: observation.receipt(),
                scope: &binding,
            },
            &parent,
        )
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(fs::read(f.a.manifest_path()).unwrap(), original);
    assert!(!destination.exists());
    assert_eq!(fs::read(&f.source).unwrap(), b"shared source");
}
