use super::*;
use crate::{
    ArtifactId, ConcreteVersion, NativeBinding, ProjectionLimits, SourceRef,
    registry::{DocumentLimits, HubLimits},
};
use graphit_core::{
    artifact_store::{ArtifactLimits, CopyLimits, CopyPolicy},
    brand::Brand,
    project_identity::{ProjectIdentity, ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use std::path::Path;

fn native_capture_request(seconds: u64) -> std::sync::Arc<graphit_core::file_io::SnapshotContext> {
    std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(seconds),
        graphit_core::file_lock::Cancellation::default(),
    ))
}

fn limits() -> HubLimits {
    HubLimits::new(
        DocumentLimits {
            bytes: 256 * 1024,
            entries: 128,
            key_bytes: 8192,
            json_nodes: 50_000,
            json_string_bytes: 32 * 1024,
        },
        ProjectionLimits::new(4096, 32 * 1024, 128, 8192, 128).unwrap(),
        ArtifactLimits::new(
            256 * 1024,
            8192,
            CopyLimits::new(16 * 1024 * 1024, 512, 32 * 1024, 16).unwrap(),
        )
        .unwrap(),
        crate::materialization::MaterializationLimits {
            output: graphit_core::artifact_store::CopyLimits::new(
                512 * 1024 * 1024,
                100_000,
                8 * 1024 * 1024,
                128,
            )
            .unwrap(),
            snapshots: graphit_core::artifact_store::CopyLimits::new(
                16 * 1024 * 1024,
                512,
                32 * 1024,
                16,
            )
            .unwrap(),
            lineage: crate::materialization::LineageLimits::new(4096, 8192, 32 * 1024).unwrap(),
        },
    )
    .unwrap()
}

fn derived(
    source: &SourceRef,
    name: &str,
    bytes: &[u8],
) -> crate::materialization::DerivedArtifact {
    derived_with_modes(source, name, bytes, None)
}
fn derived_with_modes(
    source: &SourceRef,
    name: &str,
    bytes: &[u8],
    modes: Option<(
        graphit_core::artifact_store::PayloadMode,
        graphit_core::artifact_store::PayloadMode,
    )>,
) -> crate::materialization::DerivedArtifact {
    use graphit_core::artifact_store::*;
    let context = SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    );
    let copy = limits().materialization().snapshots;
    let retention = RetainedBytesLimits::new(64 * 1024, 10, 64 * 1024).unwrap();
    let snapshot = SourceSnapshot::capture(
        source.directory().as_path(),
        graphit_fs::NodeKind::Directory,
        &SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            copy,
            RetainedFileSet::new(["COMMAND.md".into()], retention).unwrap(),
            retention,
        )
        .unwrap(),
        &context,
    )
    .unwrap();
    let root_mode = modes.map_or(snapshot.nodes().next().unwrap().mode(), |m| m.0);
    let file_mode = PayloadMode::from_permissions(
        &fs::metadata(source.directory().as_path().join("COMMAND.md"))
            .unwrap()
            .permissions(),
    )
    .unwrap();
    let file_mode = modes.map_or(file_mode, |m| m.1);
    let plan = ValidatedPayloadPlan::new(
        snapshot,
        [PayloadOverlay::new(
            "COMMAND.md".into(),
            "rendered.md".into(),
            bytes.to_vec(),
            file_mode,
        )
        .unwrap()],
        &PayloadPolicy::new(graphit_fs::NodeKind::Directory, root_mode, copy),
        &context,
    )
    .unwrap();
    crate::materialization::DerivedArtifact::new(
        source.clone(),
        plan,
        "codex-v1",
        name,
        (
            PayloadSelection::new("COMMAND.md".into(), graphit_fs::NodeKind::File).unwrap(),
            PayloadSelection::new("rendered.md".into(), graphit_fs::NodeKind::File).unwrap(),
        ),
        limits().materialization().lineage,
    )
    .unwrap()
}

#[test]
fn derived_alias_keeps_origin_and_installed_bytes_then_noop_and_source_free_remove() {
    {
        let mode = copy();
        let f = Fixture::new();
        let source = f.command("v1", "original A");
        let request = || ProjectionRequest {
            binding: NativeBinding::Codex,
            source: &source,
            alias: Some("alias-B"),
            entry_name: None,
        };
        let mut w = f.writer();
        let result = w
            .publish_derived(
                request(),
                mode,
                derived(&source, "alias-B", b"rendered B"),
                None,
            )
            .unwrap();
        assert!(result.cleanup_pending.is_none());
        assert_eq!(
            fs::read(f.destination(NativeBinding::Codex, "alias-B")).unwrap(),
            b"rendered B"
        );
        let installed = &w
            .snapshot()
            .get_for_consumer(w.consumer(), ArtifactType::Command, source.id().as_str())
            .unwrap()
            .installations[0];
        assert_eq!(installed.original_source, source);
        assert!(matches!(&installed.payload, InstalledPayload::Derived(_)));
        let result = w
            .publish_derived(
                request(),
                mode,
                derived(&source, "alias-B", b"rendered B"),
                None,
            )
            .unwrap();
        assert_eq!(result.change, PublicationChange::VerifiedUnchanged);
        fs::remove_dir_all(source.directory().as_path()).unwrap();
        drop(w);
        let mut w = f.writer();
        assert!(w.recover().unwrap().cleanup_pending.is_none());
        let key = key(NativeBinding::Codex, ArtifactType::Command);
        assert!(w.remove(&key).unwrap().cleanup_pending.is_none());
        assert!(!f.destination(NativeBinding::Codex, "alias-B").exists());
    }
}

#[test]
fn derived_commit_recovers_without_origin_and_unstarted_render_retries() {
    for mode in [copy()] {
        for boundary in [
            Boundary::Intent,
            Boundary::Parents,
            Boundary::PhysicalStarted,
            Boundary::Physical,
            Boundary::PhysicalComplete,
            Boundary::Logical,
            Boundary::Clear,
        ] {
            let f = Fixture::new();
            let source = f.command("v1", "origin");
            let mut w = f.writer();
            w.fault = Some(boundary);
            let error = w
                .publish_derived(
                    request(&source, NativeBinding::Codex),
                    mode,
                    derived(&source, "repo", b"frozen output"),
                    None,
                )
                .unwrap_err();
            assert!(error.hub_intent.is_some());
            assert_eq!(
                error.committed,
                matches!(boundary, Boundary::Logical | Boundary::Clear)
            );
            drop(w);
            let mut fresh = f.writer();
            assert!(fresh.recover().unwrap().cleanup_pending.is_none());
            assert!(fresh.snapshot().intent().is_none());
            if matches!(
                boundary,
                Boundary::Intent | Boundary::Parents | Boundary::PhysicalStarted
            ) {
                assert!(!f.destination(NativeBinding::Codex, "repo").exists());
                fresh
                    .publish_derived(
                        request(&source, NativeBinding::Codex),
                        mode,
                        derived(&source, "repo", b"frozen output"),
                        None,
                    )
                    .unwrap();
            }
            fs::remove_dir_all(source.directory().as_path()).unwrap();
            assert_eq!(
                fs::read(f.destination(NativeBinding::Codex, "repo")).unwrap(),
                b"frozen output"
            );
            assert!(
                fresh
                    .remove(&key(NativeBinding::Codex, ArtifactType::Command))
                    .unwrap()
                    .cleanup_pending
                    .is_none()
            );
        }
    }
}

#[test]
fn derived_after_clear_reports_committed_cleanup_and_fresh_sweep_finishes() {
    let f = Fixture::new();
    let source = f.command("v1", "origin");
    let mut w = f.writer();
    w.fault = Some(Boundary::Cleared);
    let result = w
        .publish_derived(
            request(&source, NativeBinding::Codex),
            copy(),
            derived(&source, "repo", b"output"),
            None,
        )
        .unwrap();
    assert!(result.cleanup_pending.is_some());
    assert!(w.snapshot().intent().is_none());
    assert_eq!(
        fs::read(f.destination(NativeBinding::Codex, "repo")).unwrap(),
        b"output"
    );
    fs::remove_dir_all(source.directory().as_path()).unwrap();
    drop(w);
    let mut fresh = f.writer();
    assert!(fresh.recover().unwrap().cleanup_pending.is_none());
    assert!(
        fresh
            .remove(&key(NativeBinding::Codex, ArtifactType::Command))
            .unwrap()
            .cleanup_pending
            .is_none()
    );
}

#[test]
fn shared_derived_tree_keeps_other_adapter_until_last_remove() {
    let f = Fixture::new();
    let source = f.command("v1", "origin");
    let mut w = f.writer();
    for binding in [NativeBinding::Codex, NativeBinding::Claude] {
        w.publish_derived(
            request(&source, binding),
            copy(),
            derived(&source, "repo", b"same output"),
            None,
        )
        .unwrap();
    }

    fs::remove_dir_all(source.directory().as_path()).unwrap();
    w.remove(&key(NativeBinding::Codex, ArtifactType::Command))
        .unwrap();

    assert_eq!(
        fs::read(f.destination(NativeBinding::Claude, "repo")).unwrap(),
        b"same output"
    );
    w.remove(&key(NativeBinding::Claude, ArtifactType::Command))
        .unwrap();
}

#[test]
fn derived_preflight_rejects_changed_origin_and_policy_without_operation_writes() {
    let f = Fixture::new();
    let source = f.command("v1", "original");
    let artifact = derived(&source, "repo", b"output");
    fs::write(source.directory().as_path().join("COMMAND.md"), "changed!").unwrap();
    let mut w = f.writer();
    assert!(
        w.publish_derived(
            request(&source, NativeBinding::Codex),
            copy(),
            artifact,
            None
        )
        .is_err()
    );
    assert!(w.snapshot().intent().is_none());
}
#[cfg(unix)]
fn derived_scope(root: &Path, global: bool) -> ScopedCoordinator {
    use graphit_core::{artifact_store::*, installation_identity::InstallationId};
    let installation = InstallationPaths::resolve(
        Brand::default(),
        root,
        None,
        Some(&root.join("installation")),
    )
    .unwrap();
    let project = ProjectPaths::at(root.join("consumer"), &installation).unwrap();
    let native = native_capture_request(30);
    let scope_limits = ScopeLimits::new(1, 16 * 1024, 1024).unwrap();
    let (registry, scope) = if global {
        fs::create_dir_all(installation.root()).unwrap();
        let id = InstallationId::new("derived-kill-fixture").unwrap();
        (
            HubRegistry::global(&installation, &id, limits()).unwrap(),
            CapturedPublicationScope::installation(
                &installation,
                id,
                [project.root().to_owned()],
                scope_limits,
                &native,
            )
            .unwrap(),
        )
    } else {
        let identity = ProjectIdentityStore::at(&project).load().unwrap().unwrap();

        (
            HubRegistry::project(&project, &identity, limits()).unwrap(),
            CapturedPublicationScope::project(&project, identity.id(), scope_limits, &native)
                .unwrap(),
        )
    };
    ScopedCoordinator::new(
        registry,
        ArtifactStore::new(scope, limits().artifacts()),
        NativePath::new(project.root().to_owned()).unwrap(),
    )
    .unwrap()
}

fn copy() -> ArtifactMode {
    ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
}
struct Fixture {
    temp: tempfile::TempDir,
    project: ProjectPaths,
    identity: ProjectIdentity,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir_in(fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let root = temp.path().join("consumer");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(installation.brand().directory())).unwrap();
        let project = ProjectPaths::at(root, &installation).unwrap();
        let identity = ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(1), &Cancellation::default())
            .unwrap()
            .ensure(&ProjectMetadata::new("consumer").unwrap())
            .unwrap();
        Self {
            temp,
            project,
            identity,
        }
    }
    fn registry(&self) -> HubRegistry {
        HubRegistry::project(&self.project, &self.identity, limits()).unwrap()
    }
    fn store(&self) -> ArtifactStore {
        ArtifactStore::new(
            graphit_core::artifact_store::CapturedPublicationScope::project(
                &self.project,
                self.identity.id(),
                graphit_core::artifact_store::ScopeLimits::new(1, 16 * 1024, 1024).unwrap(),
                &native_capture_request(5),
            )
            .unwrap(),
            limits().artifacts(),
        )
    }
    fn coordinator(&self) -> ScopedCoordinator {
        ScopedCoordinator::new(
            self.registry(),
            self.store(),
            NativePath::new(self.project.root().to_owned()).unwrap(),
        )
        .unwrap()
    }
    // Positive flows share one 30-second request across their durable writes.
    // Expiry/cancellation cases create their own short or cancelled requests.
    fn writer(&self) -> PublicationWriter {
        self.coordinator()
            .writer(&request_context(
                Duration::from_secs(30),
                &Cancellation::default(),
            ))
            .unwrap()
    }
    fn source(&self, kind: ArtifactType, version: &str) -> SourceRef {
        let path = self.temp.path().join("producer");
        fs::create_dir_all(&path).unwrap();
        SourceRef::local_link(
            NativePath::new(path).unwrap(),
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            kind,
            ArtifactId::new("org/repo", limits().projection()).unwrap(),
            ConcreteVersion::new(version, limits().projection()).unwrap(),
            limits().projection(),
        )
        .unwrap()
    }
    fn command(&self, version: &str, text: &str) -> SourceRef {
        let source = self.source(ArtifactType::Command, version);
        fs::write(source.directory().as_path().join("COMMAND.md"), text).unwrap();
        source
    }
    fn destination(&self, binding: NativeBinding, name: &str) -> std::path::PathBuf {
        self.project
            .root()
            .join(binding.adapter().layout().root)
            .join(binding.adapter().layout().commands.unwrap())
            .join(format!("{name}.md"))
    }
}
fn request(source: &SourceRef, binding: NativeBinding) -> ProjectionRequest<'_> {
    ProjectionRequest {
        binding,
        source,
        alias: None,
        entry_name: None,
    }
}
fn key(binding: NativeBinding, kind: ArtifactType) -> ArtifactKey {
    crate::InstallationTarget::for_artifact(binding, kind)
        .key(kind, "org/repo")
        .unwrap()
}

#[test]
fn first_install_verified_noop_metadata_update_and_remove_after_source_disappears() {
    let f = Fixture::new();
    let source = f.command("v1", "original");
    let dest = f.destination(NativeBinding::Codex, "repo");
    let mut w = f.writer();
    let created = w
        .publish(request(&source, NativeBinding::Codex), copy(), None)
        .unwrap();
    assert_eq!(
        created.change,
        PublicationChange::Physical(ArtifactChange::Created)
    );
    assert_eq!(created.possible_parents.len(), 2);
    assert_eq!(fs::read(&dest).unwrap(), b"original");
    let bytes = fs::read(f.registry().path()).unwrap();
    let core = fs::read(f.store().manifest_path()).unwrap();
    let noop = w
        .publish(request(&source, NativeBinding::Codex), copy(), None)
        .unwrap();
    assert_eq!(noop.change, PublicationChange::VerifiedUnchanged);

    assert_eq!(fs::read(f.registry().path()).unwrap(), bytes);
    assert_eq!(fs::read(f.store().manifest_path()).unwrap(), core);
    let v2 = f.command("feature/α", "original");
    let metadata = w
        .publish(request(&v2, NativeBinding::Codex), copy(), None)
        .unwrap();
    assert_eq!(
        metadata.change,
        PublicationChange::Physical(ArtifactChange::Unchanged)
    );

    assert_eq!(
        w.snapshot()
            .get_for_consumer(w.consumer(), ArtifactType::Command, "org/repo")
            .unwrap()
            .installations[0]
            .original_source
            .version()
            .as_str(),
        "feature/α"
    );
    fs::remove_dir_all(source.directory().as_path()).unwrap();
    w.remove(&key(NativeBinding::Codex, ArtifactType::Command))
        .unwrap();
    assert!(!dest.exists());
    assert!(w.snapshot().entries().next().is_none());
    assert!(dest.parent().unwrap().is_dir()); // Layout infrastructure is retained.
}

#[test]
fn adapter_updates_preserve_other_sources_and_order() {
    let f = Fixture::new();
    let a = f.command("v1", "a");
    let mut w = f.writer();
    for binding in [NativeBinding::Codex, NativeBinding::Cursor] {
        w.publish(request(&a, binding), copy(), None).unwrap();
    }
    let before = w
        .snapshot()
        .get_for_consumer(w.consumer(), ArtifactType::Command, "org/repo")
        .unwrap()
        .installations[1]
        .clone();
    let b = f.command("v2", "b");
    w.publish(request(&b, NativeBinding::Codex), copy(), None)
        .unwrap();
    let entry = w
        .snapshot()
        .get_for_consumer(w.consumer(), ArtifactType::Command, "org/repo")
        .unwrap();
    assert_eq!(entry.installations[1], before);
    assert_eq!(
        entry.installations[0].target,
        crate::InstallationTarget::for_artifact(NativeBinding::Codex, entry.artifact_type)
    );
    w.remove(&key(NativeBinding::Codex, ArtifactType::Command))
        .unwrap();
    assert_eq!(
        w.snapshot()
            .get_for_consumer(w.consumer(), ArtifactType::Command, "org/repo")
            .unwrap()
            .installations,
        vec![before]
    );
    assert_eq!(
        fs::read(f.destination(NativeBinding::Cursor, "repo")).unwrap(),
        b"a"
    );
}

#[test]
fn real_publish_failures_recover_same_writer_and_fresh_open_without_early_metadata() {
    for fault in [
        Boundary::Intent,
        Boundary::Parents,
        Boundary::PhysicalStarted,
        Boundary::Physical,
        Boundary::PhysicalComplete,
        Boundary::Logical,
        Boundary::Clear,
    ] {
        for reopen in [false, true] {
            let f = Fixture::new();
            let source = f.command("v1", "bytes");
            let mut w = f.writer();
            w.fault = Some(fault);
            let err = w
                .publish(request(&source, NativeBinding::Codex), copy(), None)
                .unwrap_err();
            assert!(err.hub_intent.is_some());
            if matches!(
                fault,
                Boundary::Intent | Boundary::Parents | Boundary::PhysicalStarted
            ) {
                assert!(
                    w.snapshot()
                        .get_for_consumer(w.consumer(), ArtifactType::Command, "org/repo")
                        .is_none()
                );
            }
            if reopen {
                drop(w);
                w = f.writer();
            }
            w.recover().unwrap();
            assert!(w.snapshot().intent().is_none());
            assert_eq!(
                fs::read(f.destination(NativeBinding::Codex, "repo")).unwrap(),
                b"bytes"
            );
            assert!(
                w.snapshot()
                    .get_for_consumer(w.consumer(), ArtifactType::Command, "org/repo")
                    .is_some()
            );
            assert!(w.physical_snapshot().unwrap().unwrap().pending().is_none());
        }
    }
}

#[test]
fn forget_pending_publish_and_remove_after_logical_commit_uses_committed_baseline() {
    for remove in [false, true] {
        let f = Fixture::new();
        let source = f.command("v1", "bytes");
        let mut w = f.writer();
        w.publish(request(&source, NativeBinding::Cursor), copy(), None)
            .unwrap();
        if remove {
            w.publish(request(&source, NativeBinding::Codex), copy(), None)
                .unwrap();
        }
        w.fault = Some(Boundary::Logical);
        let k = key(NativeBinding::Codex, ArtifactType::Command);
        if remove {
            w.remove(&k).unwrap_err();
        } else {
            w.publish(request(&source, NativeBinding::Codex), copy(), None)
                .unwrap_err();
        }
        assert_eq!(
            w.snapshot().intent().unwrap().phase,
            IntentPhase::LogicalCommitted
        );
        drop(w);
        let mut w = f.writer();
        fs::remove_dir_all(source.directory().as_path()).unwrap();
        let other = key(NativeBinding::Cursor, ArtifactType::Command);
        assert!(w.forget(&other).is_err());
        w.fault = Some(Boundary::Forget);
        w.forget(&k).unwrap_err();
        let preview = match &w.snapshot().intent().unwrap().goal {
            LogicalGoal::Forget { preview, previous } => {
                assert_eq!(
                    previous.as_ref().unwrap().phase,
                    IntentPhase::LogicalCommitted
                );
                preview.clone()
            }
            _ => panic!(),
        };
        drop(w);
        let mut w = f.writer();
        let result = w.recover().unwrap();
        assert_eq!(result.forget_preview.as_ref(), Some(&preview));

        assert!(w.snapshot().intent().is_none());
        assert_eq!(
            w.snapshot()
                .get_for_consumer(w.consumer(), ArtifactType::Command, "org/repo")
                .unwrap()
                .installations[0]
                .target,
            crate::InstallationTarget::for_artifact(NativeBinding::Cursor, ArtifactType::Command)
        );
        let dest = f.destination(NativeBinding::Codex, "repo");
        assert_eq!(dest.exists(), !remove);
        if !remove {
            assert_eq!(fs::read(dest).unwrap(), b"bytes");
        }
    }
}

#[test]
fn conflict_preserves_foreign_file_and_explicit_forget_does_not_require_source() {
    let f = Fixture::new();
    let source = f.command("v1", "source");
    let dest = f.destination(NativeBinding::Codex, "repo");
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    fs::write(&dest, "foreign").unwrap();
    let mut w = f.writer();
    let err = w
        .publish(request(&source, NativeBinding::Codex), copy(), None)
        .unwrap_err();
    assert!(matches!(err.failure, PublicationFailure::Physical(_)));
    assert!(err.hub_intent.is_some());
    assert_eq!(fs::read(&dest).unwrap(), b"foreign");
    fs::remove_dir_all(source.directory().as_path()).unwrap();
    assert!(w.recover().is_err());
    let forgotten = w
        .forget(&key(NativeBinding::Codex, ArtifactType::Command))
        .unwrap();
    assert_eq!(forgotten.change, PublicationChange::Forgotten);
    assert_eq!(fs::read(dest).unwrap(), b"foreign");
    assert!(w.snapshot().intent().is_none());
    assert!(w.snapshot().entries().next().is_none());
}

#[test]
fn cancellation_and_mismatched_contexts_fail_before_payload_or_locks() {
    let f = Fixture::new();
    let source = f.command("v1", "source");
    let g = Fixture::new();
    assert!(
        ScopedCoordinator::new(
            f.registry(),
            g.store(),
            NativePath::new(f.project.root().to_owned()).unwrap(),
        )
        .is_err()
    );
    let other_id = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
    assert!(
        ScopedCoordinator::new(
            f.registry(),
            ArtifactStore::new(
                graphit_core::artifact_store::CapturedPublicationScope::project(
                    &f.project,
                    other_id,
                    graphit_core::artifact_store::ScopeLimits::new(1, 16 * 1024, 1024).unwrap(),
                    &native_capture_request(5),
                )
                .unwrap(),
                limits().artifacts()
            ),
            NativePath::new(f.project.root().to_owned()).unwrap(),
        )
        .is_err()
    );
    assert!(!f.store().manifest_path().exists());
    assert!(!f.registry().path().exists());
    let service_signal = Cancellation::default();
    let service = crate::policy::RequestContext::service(
        crate::policy::HubResourcePolicy::resolve(
            &graphit_core::config::ConfigSnapshot::new(&graphit_core::brand::Brand::default(), [])
                .unwrap(),
        )
        .unwrap()
        .content(),
        &service_signal,
    )
    .unwrap();
    let error = f.coordinator().writer(&service).err().unwrap();
    assert!(
        matches!(error.failure, PublicationFailure::Local(ref error) if error.kind() == io::ErrorKind::InvalidInput)
    );
    assert!(!f.registry().lock_path().exists());
    let cancelled = Cancellation::default();
    let cancelled_request = request_context(Duration::from_secs(1), &cancelled);
    cancelled.cancel();
    assert!(f.coordinator().writer(&cancelled_request).is_err());
    assert!(!f.registry().lock_path().exists());
    assert!(!f.destination(NativeBinding::Codex, "repo").exists());
    let mut w = f.writer();
    w.cancellation.cancel();
    assert!(
        w.publish(request(&source, NativeBinding::Codex), copy(), None)
            .is_err()
    );
    assert!(w.snapshot().intent().is_none());
}

#[test]
fn dynamic_rule_has_no_file_intent_and_native_capability_is_explicitly_unsupported() {
    let f = Fixture::new();
    let rule = f.source(ArtifactType::Rule, "v1");
    fs::write(rule.directory().as_path().join("RULE.md"), "rule").unwrap();
    let mut w = f.writer();
    let r = w
        .publish(request(&rule, NativeBinding::Codex), copy(), None)
        .unwrap();
    assert_eq!(r.change, PublicationChange::MetadataOnly);

    assert!(w.snapshot().intent().is_none());
    let native = f.source(ArtifactType::Mcp, "v1");
    let r = w
        .publish(request(&native, NativeBinding::Codex), copy(), None)
        .unwrap();
    assert_eq!(r.change, PublicationChange::Unsupported);

    assert!(w.snapshot().get(ArtifactType::Mcp, "org/repo").is_none());
}

#[cfg(unix)]
#[test]
fn missing_parent_preparation_retains_modes_and_reports_interrupted_creation() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let source = f.command("v1", "bytes");
    let mut w = f.writer();
    w.fault = Some(Boundary::Parents);
    let error = w
        .publish(request(&source, NativeBinding::Codex), copy(), None)
        .unwrap_err();
    let intent = error.hub_intent.unwrap();
    let LogicalGoal::Publish { bindings } = intent.goal else {
        panic!()
    };
    assert_eq!(bindings.possible_parents.len(), 2);
    assert_eq!(
        fs::metadata(bindings.possible_parents[0].as_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(!bindings.possible_parents[1].as_path().exists());
    fs::set_permissions(
        bindings.possible_parents[0].as_path(),
        fs::Permissions::from_mode(0o750),
    )
    .unwrap();
    w.recover().unwrap();
    assert_eq!(
        fs::metadata(bindings.possible_parents[0].as_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o750
    );
    assert_eq!(
        fs::metadata(bindings.possible_parents[1].as_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[cfg(unix)]
fn link(source: &Path, destination: &Path) {
    std::os::unix::fs::symlink(source, destination).unwrap();
}

#[cfg(unix)]
#[test]
fn retargeted_source_and_destination_and_runtime_alias_fail_with_pending_data() {
    for target_source in [true, false] {
        let f = Fixture::new();
        let source = f.command("v1", "bytes");
        let mut w = f.writer();
        w.fault = Some(Boundary::Intent);
        w.publish(request(&source, NativeBinding::Codex), copy(), None)
            .unwrap_err();
        let alternate = f.temp.path().join("alternate");
        fs::create_dir(&alternate).unwrap();
        fs::write(alternate.join("COMMAND.md"), "other").unwrap();
        if target_source {
            fs::remove_dir_all(source.directory().as_path()).unwrap();
            link(&alternate, source.directory().as_path());
        } else {
            link(&alternate, &f.project.root().join(".codex"));
        }
        assert!(w.recover().is_err());
        assert!(w.snapshot().intent().is_some());
        assert!(!alternate.join("commands/repo.md").exists());
        assert!(w.snapshot().entries().next().is_none());
    }
}

#[test]
fn copy_update_detects_same_size_bytes_and_alias_relocation_removes_only_owned_old_path() {
    let f = Fixture::new();
    let source = f.command("v1", "AAAA");
    let mut w = f.writer();
    w.publish(request(&source, NativeBinding::Codex), copy(), None)
        .unwrap();
    fs::write(source.directory().as_path().join("COMMAND.md"), "BBBB").unwrap();
    let result = w
        .publish(request(&source, NativeBinding::Codex), copy(), None)
        .unwrap();
    assert_eq!(
        result.change,
        PublicationChange::Physical(ArtifactChange::Updated)
    );

    assert_eq!(
        fs::read(f.destination(NativeBinding::Codex, "repo")).unwrap(),
        b"BBBB"
    );
    w.publish(
        ProjectionRequest {
            binding: NativeBinding::Codex,
            source: &source,
            alias: Some("renamed"),
            entry_name: None,
        },
        copy(),
        None,
    )
    .unwrap();
    assert!(!f.destination(NativeBinding::Codex, "repo").exists());
    assert_eq!(
        fs::read(f.destination(NativeBinding::Codex, "renamed")).unwrap(),
        b"BBBB"
    );
}

#[cfg(unix)]
#[test]
fn real_tree_link_copy_and_selected_symlink_policy() {
    let f = Fixture::new();
    let skill = f.source(ArtifactType::Skill, "v1");
    fs::create_dir(skill.directory().as_path().join("empty")).unwrap();
    fs::write(skill.directory().as_path().join("SKILL.md"), "tree").unwrap();
    let mut w = f.writer();
    w.publish(
        request(&skill, NativeBinding::Codex),
        ArtifactMode::Link,
        None,
    )
    .unwrap();
    let dest = f.project.root().join(".agents/skills/repo");
    assert!(
        fs::symlink_metadata(&dest)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    w.publish(request(&skill, NativeBinding::Codex), copy(), None)
        .unwrap();
    assert!(
        !fs::symlink_metadata(&dest)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(dest.join("empty").is_dir());
    assert_eq!(fs::read(dest.join("SKILL.md")).unwrap(), b"tree");
    let cmd = f.command("v1", "original");
    let original = cmd.directory().as_path().join("COMMAND.md");
    let target = f.temp.path().join("target.md");
    fs::rename(&original, &target).unwrap();
    std::os::unix::fs::symlink(&target, &original).unwrap();
    assert!(
        w.publish(request(&cmd, NativeBinding::Cursor), copy(), None)
            .is_err()
    );
    assert!(w.snapshot().intent().is_none());
    w.publish(
        request(&cmd, NativeBinding::Cursor),
        ArtifactMode::Copy(CopyPolicy::FollowRegularFiles),
        None,
    )
    .unwrap();
    assert_eq!(
        fs::read(f.destination(NativeBinding::Cursor, "repo")).unwrap(),
        b"original"
    );
}

#[test]
fn current_future_document_budget_rejects_before_parent_provisioning() {
    let f = Fixture::new();
    let source = f.command("v1", "source");
    let original = limits();
    let small = HubLimits::new(
        DocumentLimits {
            bytes: 128,
            entries: 1,
            key_bytes: 8192,
            json_nodes: 1000,
            json_string_bytes: 32768,
        },
        original.projection(),
        original.artifacts(),
        crate::materialization::MaterializationLimits {
            output: graphit_core::artifact_store::CopyLimits::new(
                512 * 1024 * 1024,
                100_000,
                8 * 1024 * 1024,
                128,
            )
            .unwrap(),
            snapshots: graphit_core::artifact_store::CopyLimits::new(
                16 * 1024 * 1024,
                512,
                32 * 1024,
                16,
            )
            .unwrap(),
            lineage: crate::materialization::LineageLimits::new(4096, 8192, 32 * 1024).unwrap(),
        },
    )
    .unwrap();
    let registry = HubRegistry::project(&f.project, &f.identity, small).unwrap();
    let coordinator = ScopedCoordinator::new(
        registry,
        f.store(),
        NativePath::new(f.project.root().to_owned()).unwrap(),
    )
    .unwrap();
    let mut w = coordinator
        .writer(&request_context(
            Duration::from_secs(1),
            &Cancellation::default(),
        ))
        .unwrap();
    assert!(
        w.publish(request(&source, NativeBinding::Codex), copy(), None)
            .is_err()
    );
    assert!(!f.project.root().join(".codex").exists());
    assert!(!f.registry().path().exists());
}

#[test]
fn remove_missing_owned_preserves_foreign_and_forget_reports_real_owned_without_touch() {
    let f = Fixture::new();
    let source = f.command("v1", "source");
    let dest = f.destination(NativeBinding::Codex, "repo");
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    fs::write(&dest, "foreign").unwrap();
    let mut w = f.writer();
    assert_eq!(
        w.remove(&key(NativeBinding::Codex, ArtifactType::Command))
            .unwrap()
            .change,
        PublicationChange::MetadataOnly
    );
    assert_eq!(fs::read(&dest).unwrap(), b"foreign");
    fs::remove_file(&dest).unwrap();
    w.publish(request(&source, NativeBinding::Codex), copy(), None)
        .unwrap();
    fs::remove_dir_all(source.directory().as_path()).unwrap();
    let before = fs::metadata(&dest).unwrap();
    let result = w
        .forget(&key(NativeBinding::Codex, ArtifactType::Command))
        .unwrap();
    assert!(result.forget_preview.unwrap().owned.is_some());
    assert_eq!(fs::read(&dest).unwrap(), b"source");
    assert_eq!(
        fs::metadata(&dest).unwrap().modified().unwrap(),
        before.modified().unwrap()
    );
    assert!(w.snapshot().entries().next().is_none());
    assert!(w.physical_snapshot().unwrap().unwrap().entries().is_empty());
}

#[test]
fn cancellation_after_physical_commit_preserves_goal_for_uncancelled_reopen() {
    let f = Fixture::new();
    let source = f.command("v1", "bytes");
    let mut w = f.writer();
    w.cancel_at = Some(Boundary::Physical);
    let err = w
        .publish(request(&source, NativeBinding::Codex), copy(), None)
        .unwrap_err();
    assert!(
        matches!(err.failure,PublicationFailure::Local(ref e) if e.kind()==io::ErrorKind::Interrupted)
    );
    assert_eq!(err.hub_intent.unwrap().phase, IntentPhase::PhysicalStarted);
    assert_eq!(
        fs::read(f.destination(NativeBinding::Codex, "repo")).unwrap(),
        b"bytes"
    );
    assert!(
        w.snapshot()
            .get_for_consumer(w.consumer(), ArtifactType::Command, "org/repo")
            .is_none()
    );
    drop(w);
    let mut w = f.writer();
    w.recover().unwrap();
    assert!(w.snapshot().intent().is_none());
    assert!(
        w.snapshot()
            .get_for_consumer(w.consumer(), ArtifactType::Command, "org/repo")
            .is_some()
    );
}

#[cfg(unix)]
#[test]
fn actual_readonly_hub_failure_after_physical_commit_preserves_payload_and_intent() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let source = f.command("v1", "bytes");
    let mut w = f.writer();
    w.readonly_at = Some(Boundary::Physical);
    let err = w
        .publish(request(&source, NativeBinding::Codex), copy(), None)
        .unwrap_err();
    assert!(
        matches!(err.failure,PublicationFailure::Local(ref e) if e.kind()==io::ErrorKind::PermissionDenied)
    );
    assert_eq!(
        fs::read(f.destination(NativeBinding::Codex, "repo")).unwrap(),
        b"bytes"
    );
    assert!(
        w.snapshot()
            .get_for_consumer(w.consumer(), ArtifactType::Command, "org/repo")
            .is_none()
    );
    assert_eq!(
        w.snapshot().intent().unwrap().phase,
        IntentPhase::PhysicalStarted
    );
    assert_eq!(
        fs::metadata(f.registry().path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o400
    );
    drop(w);
    fs::set_permissions(f.registry().path(), fs::Permissions::from_mode(0o600)).unwrap();
    let mut w = f.writer();
    w.recover().unwrap();
    assert!(w.snapshot().intent().is_none());
}

#[test]
fn ordered_dual_writer_locks_timeout_without_recursive_acquisition() {
    let f = Fixture::new();
    let first = f.writer();
    let coordinator = f.coordinator();
    let start = Instant::now();
    let err = std::thread::spawn(move || {
        coordinator
            .writer(&request_context(
                Duration::from_millis(25),
                &Cancellation::default(),
            ))
            .err()
            .unwrap()
    })
    .join()
    .unwrap();
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(
        matches!(err.failure,PublicationFailure::Local(ref e) if e.kind()==io::ErrorKind::TimedOut)
    );
    drop(first);
    let core = f
        .store()
        .writer(Duration::from_secs(1), &Cancellation::default())
        .unwrap();
    assert!(
        f.coordinator()
            .writer(&request_context(
                Duration::from_millis(25),
                &Cancellation::default()
            ))
            .is_err()
    );
    // Failed acquisition released the Hub writer instead of retaining a half-coordinator.
    let hub = f
        .registry()
        .writer(Duration::from_millis(25), &Cancellation::default())
        .unwrap();
    drop(hub);
    drop(core);
    assert!(
        f.coordinator()
            .writer(&request_context(
                Duration::from_secs(1),
                &Cancellation::default()
            ))
            .is_ok()
    );
}

#[test]
fn process_coordinator_child() {
    let Some(root) = std::env::var_os("GRAPHIT_TEST_COORDINATOR_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let installation = InstallationPaths::resolve(
        Brand::default(),
        &root,
        None,
        Some(&root.join("installation")),
    )
    .unwrap();
    let project = ProjectPaths::at(root.join("consumer"), &installation).unwrap();
    let identity = ProjectIdentityStore::at(&project).load().unwrap().unwrap();
    let coordinator = ScopedCoordinator::new(
        HubRegistry::project(&project, &identity, limits()).unwrap(),
        ArtifactStore::new(
            graphit_core::artifact_store::CapturedPublicationScope::project(
                &project,
                identity.id(),
                graphit_core::artifact_store::ScopeLimits::new(1, 16 * 1024, 1024).unwrap(),
                &native_capture_request(5),
            )
            .unwrap(),
            limits().artifacts(),
        ),
        NativePath::new(project.root().to_owned()).unwrap(),
    )
    .unwrap();
    let mut writer = coordinator
        .writer(&request_context(
            Duration::from_secs(2),
            &Cancellation::default(),
        ))
        .unwrap();
    fs::write(root.join("child-ready"), "ready").unwrap();
    match std::env::var("GRAPHIT_TEST_COORDINATOR_MODE")
        .unwrap()
        .as_str()
    {
        "hold" => std::thread::sleep(Duration::from_secs(45)),
        "publish" => {
            let source = SourceRef::local_link(
                NativePath::new(root.join("producer")).unwrap(),
                "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
                ArtifactType::Skill,
                ArtifactId::new("org/repo", limits().projection()).unwrap(),
                ConcreteVersion::new("v1", limits().projection()).unwrap(),
                limits().projection(),
            )
            .unwrap();
            writer
                .publish(request(&source, NativeBinding::Codex), copy(), None)
                .unwrap();
        }
        _ => panic!("invalid child mode"),
    }
}
#[test]
fn physical_kind_transition_preserves_old_source_and_allows_another_binding() {
    let f = Fixture::new();
    let source = f.command("v1", "bytes");
    let store = f.store();
    let destination = f.destination(NativeBinding::Codex, "repo");
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    let old = f.temp.path().join("old-tree");
    fs::create_dir(&old).unwrap();
    fs::write(old.join("child"), "old").unwrap();
    store
        .writer(Duration::from_secs(1), &Cancellation::default())
        .unwrap()
        .publish(
            &key(NativeBinding::Codex, ArtifactType::Command),
            graphit_core::artifact_store::PhysicalSource::ExternalSource(&old),
            &destination,
            graphit_fs::NodeKind::Directory,
            copy(),
        )
        .unwrap();
    let coordinator = ScopedCoordinator::new(
        f.registry(),
        store,
        NativePath::new(f.project.root().to_owned()).unwrap(),
    )
    .unwrap();
    let mut writer = coordinator
        .writer(&request_context(
            Duration::from_secs(1),
            &Cancellation::default(),
        ))
        .unwrap();
    let result = writer
        .publish(request(&source, NativeBinding::Codex), copy(), None)
        .unwrap();
    assert_eq!(
        result.change,
        PublicationChange::Physical(ArtifactChange::Updated)
    );
    assert!(destination.is_file());
    assert_eq!(fs::read(&destination).unwrap(), b"bytes");
    assert!(old.join("child").is_file());
    let second = writer
        .publish(request(&source, NativeBinding::Cursor), copy(), None)
        .unwrap();
    assert_eq!(
        second.change,
        PublicationChange::Physical(ArtifactChange::Created)
    );
    let installations = &writer
        .snapshot()
        .get_for_consumer(writer.consumer(), ArtifactType::Command, "org/repo")
        .unwrap()
        .installations;
    assert_eq!(installations.len(), 2);
    for binding in [NativeBinding::Codex, NativeBinding::Cursor] {
        assert!(
            installations.iter().any(|a| a.target
                == crate::InstallationTarget::for_artifact(binding, ArtifactType::Command))
        );
        assert_eq!(fs::read(f.destination(binding, "repo")).unwrap(), b"bytes");
    }
    assert_eq!(
        writer.physical_snapshot().unwrap().unwrap().entries().len(),
        2
    );
}

#[cfg(unix)]
#[test]
fn first_install_through_captured_internal_alias_and_runtime_alias_rejection() {
    let f = Fixture::new();
    let source = f.command("v1", "bytes");
    let mut writer = f.writer();
    let shared = f.project.root().join("shared");
    fs::create_dir(&shared).unwrap();
    link(&shared, &f.project.root().join(".codex"));
    let result = writer
        .publish(request(&source, NativeBinding::Codex), copy(), None)
        .unwrap();
    assert_eq!(result.possible_parents.len(), 1);
    assert_eq!(fs::read(shared.join("commands/repo.md")).unwrap(), b"bytes");
    fs::remove_dir_all(source.directory().as_path()).unwrap();
    writer
        .remove(&key(NativeBinding::Codex, ArtifactType::Command))
        .unwrap();
    assert!(shared.join("commands").is_dir());
    let source = f.command("v2", "other");
    let runtime = f.project.project_runtime(f.identity.id(), &[]).unwrap();
    link(&runtime, &f.project.root().join(".cursor"));
    assert!(
        writer
            .publish(request(&source, NativeBinding::Cursor), copy(), None)
            .is_err()
    );
    assert!(writer.snapshot().intent().is_none());
    assert!(!runtime.join("commands").exists());
}

struct ChildGuard(std::process::Child);

#[test]
fn unsupported_kimi_command_has_no_effects() {
    let f = Fixture::new();
    let source = f.command("v1", "bytes");
    let mut writer = f.writer();
    let single = writer
        .publish(request(&source, NativeBinding::Kimi), copy(), None)
        .unwrap();
    assert_eq!(single.change, PublicationChange::Unsupported);

    assert!(!f.project.root().join(".kimi-code").exists());
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn child(f: &Fixture, mode: &str) -> ChildGuard {
    ChildGuard(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "publication::tests::process_coordinator_child",
                "--nocapture",
            ])
            .env("GRAPHIT_TEST_COORDINATOR_ROOT", f.temp.path())
            .env("GRAPHIT_TEST_COORDINATOR_MODE", mode)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    )
}
#[test]
fn actual_process_death_releases_both_ordered_locks_and_preserves_valid_documents() {
    let f = Fixture::new();
    let mut child = child(&f, "hold");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f.temp.path().join("child-ready").exists() {
        assert!(Instant::now() < deadline);
        assert!(child.0.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        f.coordinator()
            .writer(&request_context(
                Duration::from_millis(25),
                &Cancellation::default()
            ))
            .is_err()
    );
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    drop(child);
    let writer = f.writer();
    assert!(writer.snapshot().intent().is_none());
    assert!(writer.physical_snapshot().unwrap().is_none());
}

#[test]
fn real_public_core_building_after_process_death_preserves_abandoned_stage_and_recovers() {
    let f = Fixture::new();
    let source = f.source(ArtifactType::Skill, "v1");
    let buffer = vec![b'x'; 128 * 1024];
    for index in 0..64 {
        fs::write(
            source.directory().as_path().join(format!("{index:03}.bin")),
            &buffer,
        )
        .unwrap();
    }
    let mut child = child(&f, "publish");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(snapshot) = f.store().load().unwrap()
            && let Some(pending) = snapshot.pending()
            && pending.phase() == ArtifactPending::Building
        {
            let stage = pending.publication().unwrap().staging_directory().unwrap();
            // Durable intent precedes mkdir. This test needs a present residue.
            match fs::symlink_metadata(stage) {
                Ok(metadata) => {
                    assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("stage observation failed: {error}"),
            }
        }
        assert!(
            Instant::now() < deadline,
            "did not observe actual durable Core Building"
        );
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "child completed before Building observation"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    drop(child);
    let snapshot = f.store().load().unwrap().unwrap();
    let pending = snapshot
        .pending()
        .expect("kill must retain pending publication");
    assert_eq!(pending.phase(), ArtifactPending::Building);
    let stage = pending
        .publication()
        .unwrap()
        .staging_directory()
        .unwrap()
        .to_owned();
    let foreign = stage.join("foreign-residue");
    fs::write(&foreign, b"preserve foreign bytes without adoption").unwrap();
    let metadata_before = fs::metadata(&stage).unwrap();
    let names_before: Vec<_> = fs::read_dir(&stage)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let mut writer = f.writer();
    let result = writer.recover().unwrap();
    assert_eq!(
        result.change,
        PublicationChange::Physical(ArtifactChange::Created)
    );
    assert!(writer.snapshot().intent().is_none());
    assert!(
        writer
            .physical_snapshot()
            .unwrap()
            .unwrap()
            .pending()
            .is_none()
    );
    let destination = f.project.root().join(".agents/skills/repo");
    assert!(!destination.join("foreign-residue").exists());
    assert_eq!(fs::read(destination.join("000.bin")).unwrap(), buffer);
    assert!(
        writer
            .snapshot()
            .get_for_consumer(writer.consumer(), ArtifactType::Skill, "org/repo")
            .is_some()
    );
    let own = key(NativeBinding::Codex, ArtifactType::Skill);
    let result = writer.forget(&own).unwrap();
    let preview = result.forget_preview.unwrap();
    assert!(preview.pending.is_none());
    assert!(preview.owned.is_some());
    assert!(stage.is_dir());
    assert_eq!(
        fs::metadata(&stage).unwrap().modified().unwrap(),
        metadata_before.modified().unwrap()
    );
    let names_after: Vec<_> = fs::read_dir(&stage)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names_before, names_after);
    assert_eq!(
        fs::read(foreign).unwrap(),
        b"preserve foreign bytes without adoption"
    );
    assert_eq!(fs::read(destination.join("000.bin")).unwrap(), buffer);
    assert!(
        writer
            .physical_snapshot()
            .unwrap()
            .unwrap()
            .pending()
            .is_none()
    );
    assert!(writer.snapshot().intent().is_none());
}

#[test]
fn derived_unsupported_is_lexical_even_after_origin_disappears() {
    let f = Fixture::new();
    let source = f.command("v1", "original");
    let artifact = derived(&source, "repo", b"rendered");
    let mut writer = f.writer();
    fs::remove_dir_all(source.directory().as_path()).unwrap();
    let unsupported = writer
        .publish_derived(
            request(&source, NativeBinding::Kimi),
            copy(),
            artifact,
            None,
        )
        .unwrap();
    assert_eq!(unsupported.change, PublicationChange::Unsupported);
}

#[test]
fn derived_changed_root_with_same_bytes_and_hardlinked_selected_file_has_no_effects() {
    let f = Fixture::new();
    let source = f.command("v1", "unchanged bytes");
    let artifact = derived(&source, "repo", b"output");
    let original = source.directory().as_path();
    let moved = f.temp.path().join("moved-producer");
    fs::rename(original, &moved).unwrap();
    fs::create_dir(original).unwrap();
    fs::hard_link(moved.join("COMMAND.md"), original.join("COMMAND.md")).unwrap();
    let mut writer = f.writer();
    assert!(
        writer
            .publish_derived(
                request(&source, NativeBinding::Codex),
                copy(),
                artifact,
                None
            )
            .is_err()
    );
    assert!(writer.physical_snapshot().unwrap().is_none());
    assert!(!f.project.root().join(".codex").exists());
}

#[test]
fn derived_deadline_fails_without_operation_journals() {
    let f = Fixture::new();
    let source = f.command("v1", "original");
    let artifact = derived(&source, "repo", b"output");
    let mut writer = f.writer();
    writer.deadline = Instant::now();
    assert!(
        writer
            .publish_derived(
                request(&source, NativeBinding::Codex),
                copy(),
                artifact,
                None
            )
            .is_err()
    );
    assert!(writer.physical_snapshot().unwrap().is_none());
    assert!(writer.snapshot().intent().is_none());
}

#[test]
fn derived_persisted_metadata_is_closed_and_bad_fingerprint_never_opens_a_writer() {
    let f = Fixture::new();
    let source = f.command("v1", "original");
    let mut writer = f.writer();
    writer
        .publish_derived(
            request(&source, NativeBinding::Codex),
            copy(),
            derived(&source, "repo", b"output"),
            None,
        )
        .unwrap();
    drop(writer);
    let hub_path = f.registry().path().to_owned();
    let original = fs::read_to_string(&hub_path).unwrap();
    let physical_before = fs::read(f.store().manifest_path()).unwrap();
    let fingerprint = derived(&source, "repo", b"output")
        .lineage()
        .input_fingerprint()
        .to_owned();
    let field = format!("\"source_fingerprint\":\"{fingerprint}\"");
    assert!(original.contains(&field));
    let malformed = [
        original.replace(
            &field,
            &format!("{field},\"source_fingerprint\":\"{fingerprint}\""),
        ),
        original.replace(&field, "\"source_fingerprint\":null"),
        original.replace(
            &field,
            &format!("\"unknown_source_fingerprint\":\"{fingerprint}\""),
        ),
        original.replace(
            &field,
            &format!("\"source_fingerprint\":\"{}\"", "A".repeat(64)),
        ),
        original.replace(
            &field,
            &format!("\"source_fingerprint\":\"{}\"", "0".repeat(63)),
        ),
        format!("{original} true"),
    ];
    for bytes in malformed {
        assert_ne!(bytes, original);
        fs::write(&hub_path, &bytes).unwrap();
        assert!(
            f.coordinator()
                .writer(&request_context(
                    Duration::from_secs(30),
                    &Cancellation::default()
                ))
                .is_err()
        );
        assert_eq!(fs::read_to_string(&hub_path).unwrap(), bytes);
        assert_eq!(
            fs::read(f.store().manifest_path()).unwrap(),
            physical_before
        );
        assert_eq!(
            fs::read(f.destination(NativeBinding::Codex, "repo")).unwrap(),
            b"output"
        );
    }
    fs::write(&hub_path, original).unwrap();
    assert!(f.writer().snapshot().intent().is_none());
}

#[cfg(unix)]
#[test]
fn derived_unicode_asset_binding_reopens_with_exact_binary_contents() {
    let f = Fixture::new();
    let source = f.command("v1", "original");
    let name = std::ffi::OsString::from("a-界-b");
    fs::write(source.directory().as_path().join(&name), [0, 0xff, 0x80]).unwrap();
    let artifact = derived(&source, "repo", b"output");
    let input_before = artifact.lineage().input_fingerprint().to_owned();
    let mut writer = f.writer();
    writer.fault = Some(Boundary::Physical);
    writer
        .publish_derived(
            request(&source, NativeBinding::Codex),
            copy(),
            artifact,
            None,
        )
        .unwrap_err();
    fs::remove_dir_all(source.directory().as_path()).unwrap();
    drop(writer);
    let mut fresh = f.writer();
    fresh.recover().unwrap();
    let installed = &fresh
        .snapshot()
        .get_for_consumer(fresh.consumer(), ArtifactType::Command, "org/repo")
        .unwrap()
        .installations[0];
    let InstalledPayload::Derived(payload) = &installed.payload else {
        panic!("expected derived");
    };
    assert_eq!(payload.source_fingerprint, input_before);
    assert_eq!(
        fs::read(f.destination(NativeBinding::Codex, "repo")).unwrap(),
        b"output"
    );
    fresh
        .remove(&key(NativeBinding::Codex, ArtifactType::Command))
        .unwrap();
}

#[cfg(unix)]
#[test]
fn derived_restricted_output_modes_are_restored_in_both_owners_then_source_free_remove_and_forget()
{
    use graphit_core::artifact_store::PayloadMode;
    use std::os::unix::fs::PermissionsExt;
    for global in [false, true] {
        for forget in [false, true] {
            let f = Fixture::new();
            let source = f.command("v1", "original");
            let modes = (
                PayloadMode::from_permissions(&fs::Permissions::from_mode(0o0)).unwrap(),
                PayloadMode::from_permissions(&fs::Permissions::from_mode(0o204)).unwrap(),
            );
            let coordinator = derived_scope(f.temp.path(), global);
            let mut writer = coordinator
                .writer(&request_context(
                    Duration::from_secs(30),
                    &Cancellation::default(),
                ))
                .unwrap();
            writer
                .publish_derived(
                    request(&source, NativeBinding::Codex),
                    copy(),
                    derived_with_modes(&source, "repo", b"output", Some(modes)),
                    None,
                )
                .unwrap();
            let destination = f.destination(NativeBinding::Codex, "repo");
            assert_eq!(
                fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
                0o204
            );
            assert!(
                writer
                    .physical_snapshot()
                    .unwrap()
                    .unwrap()
                    .pending()
                    .is_none()
            );
            fs::remove_dir_all(source.directory().as_path()).unwrap();
            drop(writer);
            let mut fresh = derived_scope(f.temp.path(), global)
                .writer(&request_context(
                    Duration::from_secs(30),
                    &Cancellation::default(),
                ))
                .unwrap();
            if forget {
                fresh
                    .forget(&key(NativeBinding::Codex, ArtifactType::Command))
                    .unwrap();
                assert_eq!(
                    fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
                    0o204
                );
                // Forget leaves the private copy as an untracked artifact; observe it explicitly.
                fs::set_permissions(&destination, fs::Permissions::from_mode(0o604)).unwrap();
                assert_eq!(fs::read(&destination).unwrap(), b"output");
                fs::set_permissions(&destination, fs::Permissions::from_mode(0o204)).unwrap();
            } else {
                fresh
                    .remove(&key(NativeBinding::Codex, ArtifactType::Command))
                    .unwrap();
                assert!(!destination.exists());
            }
            assert!(fresh.snapshot().intent().is_none());
        }
    }
}

// Exercise the actual RC renderer, MP payload owner, Hub journal and Core receipts together.
fn native_payload(
    source: &SourceRef,
    binding: NativeBinding,
    name: &str,
) -> crate::materialization::DerivedArtifact {
    use crate::native::*;
    let cancellation = Cancellation::default();
    let mut context = crate::policy::RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 16,
            max_nodes: 4096,
            max_scalar_bytes: 16384,
            max_collection_entries: 128,
        },
        &cancellation,
        Instant::now() + Duration::from_secs(10),
    )
    .unwrap();
    native_payload_in_context(source, binding, name, &mut context)
}
fn native_payload_in_context(
    source: &SourceRef,
    binding: NativeBinding,
    name: &str,
    context: &mut crate::policy::RequestContext<'_>,
) -> crate::materialization::DerivedArtifact {
    let template = if source.artifact_type() == ArtifactType::Agent {
        "AGENT.md"
    } else {
        "SKILL.md"
    };
    native_payload_from_entry(source, binding, name, context, template)
}
fn native_payload_from_entry(
    source: &SourceRef,
    binding: NativeBinding,
    name: &str,
    context: &mut crate::policy::RequestContext<'_>,
    template: &str,
) -> crate::materialization::DerivedArtifact {
    use crate::native::*;
    use graphit_core::artifact_store::*;
    use graphit_fs::NodeKind;
    let copy = limits().materialization().snapshots;
    let retention = RetainedBytesLimits::new(65536, 10, 65536).unwrap();
    let snapshot = SourceSnapshot::capture(
        source.directory().as_path(),
        NodeKind::Directory,
        &SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            copy,
            RetainedFileSet::new([template.into()], retention).unwrap(),
            retention,
        )
        .unwrap(),
        context.snapshot(),
    )
    .unwrap();
    let input = snapshot.retained(std::path::Path::new(template)).unwrap();
    let (rendered, schema) = if source.artifact_type() == ArtifactType::Agent {
        let descriptor = context.parse_agent(input).unwrap();
        let names: &[&str] = if binding == NativeBinding::Codex {
            &[]
        } else {
            &["read"]
        };
        let grants = ToolGrants {
            binding,
            read_only: true,
            allows_shell: false,
            tools: names
                .iter()
                .map(|name| NativeToolGrant {
                    name: (*name).into(),
                    access: ToolAccess::Read,
                })
                .collect(),
        };
        (
            context
                .render_agent(binding, &descriptor, Some(name), &grants)
                .unwrap(),
            binding.schema(),
        )
    } else {
        let descriptor = context.parse_skill(input).unwrap();
        (
            context
                .render_skill(binding, &descriptor, Some(name))
                .unwrap(),
            "kiro-skill-md",
        )
    };
    let root_mode = snapshot.nodes().next().unwrap().mode();
    let mode = PayloadMode::from_permissions(
        &fs::metadata(source.directory().as_path().join(template))
            .unwrap()
            .permissions(),
    )
    .unwrap();
    let output = if source.artifact_type() == ArtifactType::Agent {
        format!("rendered.{}", binding.agent_extension())
    } else {
        "SKILL.md".into()
    };
    let plan = ValidatedPayloadPlan::new(
        snapshot,
        [
            PayloadOverlay::new(template.into(), output.clone().into(), rendered.bytes, mode)
                .unwrap(),
        ],
        &PayloadPolicy::new(NodeKind::Directory, root_mode, copy),
        context.snapshot(),
    )
    .unwrap();
    let selection = if source.artifact_type() == ArtifactType::Agent {
        PayloadSelection::new(output.into(), NodeKind::File).unwrap()
    } else {
        PayloadSelection::new("".into(), NodeKind::Directory).unwrap()
    };
    crate::materialization::DerivedArtifact::new(
        source.clone(),
        plan,
        schema,
        name,
        (
            if source.artifact_type() == ArtifactType::Skill {
                PayloadSelection::new("".into(), NodeKind::Directory).unwrap()
            } else {
                PayloadSelection::new(template.into(), NodeKind::File).unwrap()
            },
            selection,
        ),
        limits().materialization().lineage,
    )
    .unwrap()
}
fn kiro_agent(f: &Fixture, version: &str, text: &str) -> SourceRef {
    let source = f.source(ArtifactType::Agent, version);
    fs::write(
        source.directory().as_path().join("AGENT.md"),
        format!("---\nname: repo\ndescription: Inspect\nread_only: true\n---\n{text}\n"),
    )
    .unwrap();
    source
}
fn installation(w: &PublicationWriter, binding: NativeBinding) -> Installation {
    w.snapshot()
        .get_for_consumer(w.consumer(), ArtifactType::Agent, "org/repo")
        .unwrap()
        .installations
        .iter()
        .find(|i| i.target == crate::InstallationTarget::for_artifact(binding, ArtifactType::Agent))
        .unwrap()
        .clone()
}
fn kiro_key(binding: NativeBinding, kind: ArtifactType) -> ArtifactKey {
    crate::InstallationTarget::for_artifact(binding, kind)
        .key(kind, "org/repo")
        .unwrap()
}
#[test]
fn derived_original_selection_preserves_arbitrary_descriptor_and_rejects_invalid_records() {
    use graphit_core::artifact_store::PayloadSelection;
    use graphit_fs::NodeKind;
    let f = Fixture::new();
    let source = kiro_agent(&f, "v1", "original instructions");
    fs::rename(
        source.directory().as_path().join("AGENT.md"),
        source.directory().as_path().join("custom-agent.md"),
    )
    .unwrap();
    let cancellation = Cancellation::default();
    let mut context = crate::policy::RequestContext::new(
        crate::native::ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 32,
        },
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let artifact = native_payload_from_entry(
        &source,
        NativeBinding::KiroV3,
        "repo",
        &mut context,
        "custom-agent.md",
    );
    assert_eq!(
        artifact.original_selection().relative(),
        Path::new("custom-agent.md")
    );
    assert_eq!(artifact.selection().relative(), Path::new("rendered.md"));
    assert_eq!(
        artifact.original_binding().link_policy(),
        CopyPolicy::RejectFileLinks
    );
    assert!(
        artifact
            .original_binding()
            .contains_node(Path::new("custom-agent.md"), NodeKind::File)
    );
    let mut writer = f.writer();
    writer
        .publish_derived(
            request(&source, NativeBinding::KiroV3),
            copy(),
            artifact,
            None,
        )
        .unwrap();
    let expected = writer.snapshot().clone();
    drop(writer);
    let registry = f.registry();
    assert_eq!(registry.load().unwrap(), expected);
    let bytes = fs::read(registry.path()).unwrap();
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let kind = serde_json::to_value(ArtifactType::Agent).unwrap();
    let pointer = format!(
        "/artifacts/{}/org~1repo/0/installations/0/payload/data",
        kind.as_str().unwrap(),
    );
    for selection in [
        None,
        Some(
            serde_json::to_value(
                PayloadSelection::new("custom-agent.md".into(), NodeKind::Directory).unwrap(),
            )
            .unwrap(),
        ),
    ] {
        let mut bad = document.clone();
        let data = bad.pointer_mut(&pointer).unwrap().as_object_mut().unwrap();
        if let Some(selection) = selection {
            data.insert("original_selection".into(), selection);
        } else {
            data.remove("original_selection");
        }
        fs::write(registry.path(), serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(registry.load().is_err());
    }
    let mut bad = document;
    bad.pointer_mut(&format!("{pointer}/original_selection/relative"))
        .unwrap()
        .clone_from(&serde_json::json!("../escape.md"));
    fs::write(registry.path(), serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(registry.load().is_err());
    fs::write(registry.path(), bytes).unwrap();
    assert_eq!(registry.load().unwrap(), expected);
}

#[test]
fn derived_original_selection_must_exist_with_the_selected_kind_in_real_capture() {
    use graphit_core::artifact_store::PayloadSelection;
    use graphit_fs::NodeKind;
    for input in [
        PayloadSelection::new("missing.md".into(), NodeKind::File).unwrap(),
        PayloadSelection::new("COMMAND.md".into(), NodeKind::Directory).unwrap(),
    ] {
        let f = Fixture::new();
        let source = f.command("v1", "original");
        let artifact = derived(&source, "repo", b"rendered");
        let result = crate::materialization::DerivedArtifact::new(
            artifact.original_source,
            artifact.plan,
            "codex-v1",
            "repo",
            (input, artifact.selection),
            limits().materialization().lineage,
        );
        assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::InvalidInput));
    }
    assert!(PayloadSelection::new("../escape.md".into(), NodeKind::File).is_err());
    // A real captured directory alone cannot stand in for a Skill descriptor.
    let f = Fixture::new();
    let command = f.command("v1", "not a skill");
    let artifact = derived(&command, "repo", b"rendered");
    let skill = SourceRef::local_link(
        command.directory().clone(),
        command.producer(),
        ArtifactType::Skill,
        command.id().clone(),
        command.version().clone(),
        limits().projection(),
    )
    .unwrap();
    let result = crate::materialization::DerivedArtifact::new(
        skill,
        artifact.plan,
        "codex-v1",
        "repo",
        (
            PayloadSelection::new("".into(), NodeKind::Directory).unwrap(),
            artifact.selection,
        ),
        limits().materialization().lineage,
    );
    assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::InvalidInput));
}

#[test]
fn current_kiro_and_codex_update_fault_recovery_remove_and_forget_preserve_other_binding() {
    for mode in [copy()] {
        for changed in [NativeBinding::KiroV3, NativeBinding::Codex] {
            for boundary in [
                Boundary::Intent,
                Boundary::PhysicalStarted,
                Boundary::Physical,
                Boundary::PhysicalComplete,
                Boundary::Logical,
                Boundary::Clear,
            ] {
                let f = Fixture::new();
                let source = kiro_agent(&f, "v1", "original A");
                let mut w = f.writer();
                let original = std::sync::Arc::clone(&w.context);
                let deadline = original.deadline();
                for binding in [NativeBinding::KiroV3, NativeBinding::Codex] {
                    w.publish_derived(
                        request(&source, binding),
                        mode,
                        native_payload(&source, binding, "repo"),
                        None,
                    )
                    .unwrap();
                }
                let other = if changed == NativeBinding::KiroV3 {
                    NativeBinding::Codex
                } else {
                    NativeBinding::KiroV3
                };
                let before = installation(&w, other);
                let path =
                    |binding: NativeBinding| f.project.root().join(binding.agent_path("repo"));
                let other_bytes = fs::read(path(other)).unwrap();
                assert!(
                    fs::read_to_string(path(NativeBinding::KiroV3))
                        .unwrap()
                        .starts_with("---\n")
                );
                let codex = fs::read_to_string(path(NativeBinding::Codex)).unwrap();
                let codex = codex.parse::<toml_edit::DocumentMut>().unwrap();
                assert!(
                    codex["developer_instructions"]
                        .as_str()
                        .unwrap()
                        .contains("original A")
                );

                let updated = kiro_agent(&f, "v2", "updated B");
                w.fault = Some(boundary);
                let error = w
                    .publish_derived(
                        request(&updated, changed),
                        mode,
                        native_payload(&updated, changed, "repo"),
                        None,
                    )
                    .unwrap_err();
                assert!(w.fault.is_none(), "publication boundary must be reached");
                assert!(
                    matches!(error.failure, PublicationFailure::Local(ref e) if e.kind() == io::ErrorKind::Other),
                    "expected injected publication boundary error, got {error:?}"
                );
                assert!(std::sync::Arc::ptr_eq(&w.context, &original));
                assert_eq!(w.context.deadline(), deadline);
                assert_eq!(Some(w.deadline), deadline);
                assert_eq!(
                    error.hub_intent.as_ref().unwrap().key.adapter(),
                    changed.as_str()
                );
                fs::remove_dir_all(source.directory().as_path()).unwrap();
                drop(w);
                let mut w = f.writer();
                assert!(w.recover().unwrap().cleanup_pending.is_none());
                assert!(w.snapshot().intent().is_none());
                assert_eq!(installation(&w, other), before);
                assert_eq!(fs::read(path(other)).unwrap(), other_bytes);
                let unstarted = matches!(boundary, Boundary::Intent | Boundary::PhysicalStarted);
                assert!(
                    fs::read_to_string(path(changed))
                        .unwrap()
                        .contains(if unstarted { "original A" } else { "updated B" })
                );
                assert_eq!(
                    installation(&w, changed).original_source.version().as_str(),
                    if unstarted { "v1" } else { "v2" }
                );

                w.remove(&kiro_key(changed, ArtifactType::Agent)).unwrap();
                assert!(!path(changed).exists());
                assert_eq!(installation(&w, other), before);

                w.forget(&kiro_key(other, ArtifactType::Agent)).unwrap();
                assert_eq!(fs::read(path(other)).unwrap(), other_bytes);
                assert!(
                    w.snapshot()
                        .get_for_consumer(w.consumer(), ArtifactType::Agent, "org/repo")
                        .is_none()
                );
            }
        }
    }
}

#[test]
fn invalid_engine_surface_keys_and_wrong_agent_schema_fail_before_effects() {
    let f = Fixture::new();
    let source = kiro_agent(&f, "v1", "original");
    let mut w = f.writer();
    for (namespace, kind) in [
        ("kiro", "agent"),
        ("kiro-v2", "skill"),
        ("kiro-v3-preview", "skill"),
        ("KIRO-V2", "agent"),
    ] {
        let key = ArtifactKey::new(namespace, kind, "org/repo").unwrap();
        assert!(w.remove(&key).is_err());
        assert!(w.forget(&key).is_err());
    }
    let wrong = native_payload(&source, NativeBinding::KiroV3, "repo");
    assert!(
        w.publish_derived(request(&source, NativeBinding::Codex), copy(), wrong, None)
            .is_err()
    );
    assert!(w.snapshot().intent().is_none());
    assert!(!f.project.root().join(".kiro").exists());
}

#[test]
fn current_kiro_and_codex_forget_pending_binding_preserves_sibling() {
    for mode in [copy()] {
        for changed in [NativeBinding::KiroV3, NativeBinding::Codex] {
            let f = Fixture::new();
            let source = kiro_agent(&f, "v1", "original A");
            let mut w = f.writer();
            let original = std::sync::Arc::clone(&w.context);
            let deadline = original.deadline();
            for binding in [NativeBinding::KiroV3, NativeBinding::Codex] {
                w.publish_derived(
                    request(&source, binding),
                    mode,
                    native_payload(&source, binding, "repo"),
                    None,
                )
                .unwrap();
            }
            let other = if changed == NativeBinding::KiroV3 {
                NativeBinding::Codex
            } else {
                NativeBinding::KiroV3
            };
            let sibling = installation(&w, other);
            let sibling_path = f.project.root().join(other.agent_path("repo"));
            let sibling_bytes = fs::read(&sibling_path).unwrap();
            let update = kiro_agent(&f, "v2", "updated B");
            w.fault = Some(Boundary::Logical);
            let error = w
                .publish_derived(
                    request(&update, changed),
                    mode,
                    native_payload(&update, changed, "repo"),
                    None,
                )
                .unwrap_err();
            assert!(w.fault.is_none(), "publication boundary must be reached");
            assert!(
                matches!(error.failure, PublicationFailure::Local(ref e) if e.kind() == io::ErrorKind::Other),
                "expected injected publication boundary error, got {error:?}"
            );
            assert!(std::sync::Arc::ptr_eq(&w.context, &original));
            assert_eq!(w.context.deadline(), deadline);
            assert_eq!(Some(w.deadline), deadline);
            fs::remove_dir_all(source.directory().as_path()).unwrap();
            drop(w);
            let mut w = f.writer();
            assert!(w.forget(&kiro_key(other, ArtifactType::Agent)).is_err());
            w.forget(&kiro_key(changed, ArtifactType::Agent)).unwrap();
            assert!(w.snapshot().intent().is_none());
            assert_eq!(
                w.snapshot()
                    .get_for_consumer(w.consumer(), ArtifactType::Agent, "org/repo")
                    .unwrap()
                    .installations,
                vec![sibling.clone()]
            );
            assert_eq!(fs::read(&sibling_path).unwrap(), sibling_bytes);
            assert!(
                fs::read_to_string(f.project.root().join(changed.agent_path("repo")))
                    .unwrap()
                    .contains("updated B")
            );
            let physical = w.physical_snapshot().unwrap().unwrap();
            assert_eq!(physical.entries().len(), 1);
            assert_eq!(
                physical.entries()[0].key(),
                &kiro_key(other, ArtifactType::Agent)
            );
            drop(w);
            assert_eq!(installation(&f.writer(), other), sibling);
        }
    }
}

#[test]
fn raw_native_agent_and_wrong_schema_have_no_effects() {
    let f = Fixture::new();
    let source = kiro_agent(&f, "v1", "original");
    fs::write(
        source.directory().as_path().join("AGENT.md"),
        "invalid raw native content",
    )
    .unwrap();
    let mut w = f.writer();
    for binding in NativeBinding::ALL {
        let result = w.publish(request(&source, binding), copy(), None).unwrap();
        assert_eq!(result.change, PublicationChange::Unsupported);
    }
    assert!(w.snapshot().entries().next().is_none());
    let source = kiro_agent(&f, "v1", "original");
    let payload = native_payload(&source, NativeBinding::KiroV3, "repo");
    assert!(
        w.publish_derived(
            request(&source, NativeBinding::Codex),
            copy(),
            payload,
            None
        )
        .is_err()
    );
    assert!(w.snapshot().intent().is_none());
    assert!(!f.project.root().join(".kiro").exists());
    assert!(!f.project.root().join(".codex").exists());
}

#[test]
fn unsupported_or_invalid_native_request_does_not_recover_preexisting_pending() {
    let f = Fixture::new();
    let command = f.command("v1", "valid pending command");
    let mut w = f.writer();
    w.fault = Some(Boundary::Intent);
    w.publish(request(&command, NativeBinding::Codex), copy(), None)
        .unwrap_err();
    let before = w.snapshot().clone();
    let bytes = fs::read(f.registry().path()).unwrap();
    let agent = kiro_agent(&f, "v1", "valid agent");
    {
        let binding = NativeBinding::KiroV3;
        let result = w.publish(request(&command, binding), copy(), None).unwrap();
        assert_eq!(result.change, PublicationChange::Unsupported);
    }
    let mcp = f.source(ArtifactType::Mcp, "v1");
    assert_eq!(
        w.publish(request(&mcp, NativeBinding::Codex), copy(), None)
            .unwrap()
            .change,
        PublicationChange::Unsupported
    );
    let wrong = native_payload(&agent, NativeBinding::KiroV3, "repo");
    assert!(
        w.publish_derived(request(&agent, NativeBinding::Codex), copy(), wrong, None)
            .is_err()
    );
    assert_eq!(
        w.publish(request(&agent, NativeBinding::KiroV3), copy(), None)
            .unwrap()
            .change,
        PublicationChange::Unsupported
    );
    assert_eq!(w.snapshot(), &before);
    assert_eq!(fs::read(f.registry().path()).unwrap(), bytes);
    assert!(!f.destination(NativeBinding::Codex, "repo").exists());
    w.recover().unwrap();
    assert_eq!(
        fs::read(f.destination(NativeBinding::Codex, "repo")).unwrap(),
        b"valid pending command"
    );
    assert!(w.snapshot().intent().is_none());
}

fn request_context<'a>(
    timeout: Duration,
    cancellation: &'a Cancellation,
) -> crate::policy::RequestContext<'a> {
    crate::policy::RequestContext::new(
        crate::native::ContentLimits {
            max_input_bytes: 8 * 1024 * 1024,
            max_output_bytes: 16 * 1024 * 1024,
            max_depth: 32,
            max_nodes: 100_000,
            max_scalar_bytes: 1024 * 1024,
            max_collection_entries: 10_000,
        },
        cancellation,
        Instant::now().checked_add(timeout).unwrap(),
    )
    .unwrap()
}

#[test]
fn original_request_is_reused_for_capture_render_and_publication() {
    use crate::policy::HubResourcePolicy;
    use graphit_core::config::ConfigSnapshot;
    for mode in [copy()] {
        for binding in [NativeBinding::KiroV3, NativeBinding::Codex] {
            let f = Fixture::new();
            let source = kiro_agent(&f, "v1", "maintained capability");
            let policy =
                HubResourcePolicy::resolve(&ConfigSnapshot::new(&Brand::default(), []).unwrap())
                    .unwrap();
            let registry = HubRegistry::project(&f.project, &f.identity, policy.hub()).unwrap();
            let store = ArtifactStore::new(f.store().scope().clone(), policy.artifacts());
            let coordinator = ScopedCoordinator::new(
                registry,
                store,
                NativePath::new(f.project.root().to_owned()).unwrap(),
            )
            .unwrap();
            let cancel = Cancellation::default();
            let mut context = policy.request_context(Instant::now(), &cancel).unwrap();
            let artifact = native_payload_in_context(&source, binding, "repo", &mut context);
            let mut writer = coordinator.writer(&context).unwrap();
            assert!(std::sync::Arc::ptr_eq(&writer.context, context.snapshot()));
            let outcome = writer
                .publish_derived(request(&source, binding), mode, artifact, None)
                .unwrap();
            assert!(outcome.cleanup_pending.is_none());
            writer.remove(&key(binding, ArtifactType::Agent)).unwrap();
        }
    }
}

#[test]
fn expired_captured_request_fails_before_creating_either_lock() {
    let f = Fixture::new();
    let cancellation = Cancellation::default();
    let context = request_context(Duration::from_millis(2), &cancellation);
    std::thread::sleep(Duration::from_millis(3));
    let error = f.coordinator().writer(&context).err().unwrap();
    assert!(
        matches!(error.failure, PublicationFailure::Local(ref cause) if cause.kind() == io::ErrorKind::TimedOut)
    );
    assert!(!f.registry().lock_path().exists());
    assert!(!f.store().manifest_path().exists());
}

#[test]
fn generated_links_are_rejected_before_creating_an_installation_or_intent() {
    let f = Fixture::new();
    let source = f.command("v1", "source");
    let mut writer = f.writer();
    let error = writer
        .publish_derived(
            request(&source, NativeBinding::Codex),
            ArtifactMode::Link,
            derived(&source, "repo", b"generated"),
            None,
        )
        .unwrap_err();
    assert!(
        matches!(error.failure, PublicationFailure::Local(ref e) if e.kind() == io::ErrorKind::InvalidInput)
    );
    assert!(writer.snapshot().intent().is_none());
    assert!(writer.physical_snapshot().unwrap().is_none());
    assert!(!f.destination(NativeBinding::Codex, "repo").exists());
}

#[test]
fn generated_installation_hash_preserves_user_changes_without_a_content_cache() {
    let f = Fixture::new();
    let source = f.command("v1", "source");
    let destination = f.destination(NativeBinding::Codex, "repo");
    let mut writer = f.writer();
    writer
        .publish_derived(
            request(&source, NativeBinding::Codex),
            copy(),
            derived(&source, "repo", b"generated"),
            None,
        )
        .unwrap();
    let record = serde_json::to_value(writer.snapshot()).unwrap();
    let record = record.to_string();
    assert!(!record.contains("original_binding"));
    assert!(!record.contains("pin_id"));
    assert!(!record.contains("generated"));
    assert!(!f.store().runtime_root().join("owned-payloads").exists());
    fs::write(&destination, b"user edit").unwrap();
    assert!(
        writer
            .publish_derived(
                request(&source, NativeBinding::Codex),
                copy(),
                derived(&source, "repo", b"next generated"),
                None,
            )
            .is_err()
    );
    assert_eq!(fs::read(&destination).unwrap(), b"user edit");
    // Explicit forget removes only ownership; the user's installation survives.
    writer
        .forget(&key(NativeBinding::Codex, ArtifactType::Command))
        .unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"user edit");
}

#[test]
fn compact_render_provenance_keeps_the_source_copy_policy() {
    use graphit_core::artifact_store::*;
    let f = Fixture::new();
    let source = f.command("v1", "source descriptor");
    let capture = native_capture_request(30);
    let snapshot = SourceSnapshot::capture(
        source.directory().as_path(),
        graphit_fs::NodeKind::Directory,
        &SnapshotPolicy::new(
            CopyPolicy::FollowRegularFiles,
            limits().materialization().snapshots,
            RetainedFileSet::new([], RetainedBytesLimits::new(8192, 8, 8192).unwrap()).unwrap(),
            RetainedBytesLimits::new(8192, 8, 8192).unwrap(),
        )
        .unwrap(),
        &capture,
    )
    .unwrap();
    let root_mode = snapshot.nodes().next().unwrap().mode();
    let plan = ValidatedPayloadPlan::new(
        snapshot,
        [],
        &PayloadPolicy::new(
            graphit_fs::NodeKind::Directory,
            root_mode,
            limits().materialization().output,
        ),
        &capture,
    )
    .unwrap();
    let artifact = crate::materialization::DerivedArtifact::new(
        source.clone(),
        plan,
        "codex-v1",
        "repo",
        (
            PayloadSelection::new("COMMAND.md".into(), graphit_fs::NodeKind::File).unwrap(),
            PayloadSelection::new("COMMAND.md".into(), graphit_fs::NodeKind::File).unwrap(),
        ),
        limits().materialization().lineage,
    )
    .unwrap();
    let mut writer = f.writer();
    writer
        .publish_derived(
            request(&source, NativeBinding::Codex),
            copy(),
            artifact,
            None,
        )
        .unwrap();
    drop(writer);
    let writer = f.writer();
    let installation = &writer
        .snapshot()
        .get_for_consumer(writer.consumer(), ArtifactType::Command, "org/repo")
        .unwrap()
        .installations[0];
    let InstalledPayload::Derived(payload) = &installation.payload else {
        panic!("expected render provenance")
    };
    assert_eq!(payload.source_policy, CopyPolicy::FollowRegularFiles);
    let encoded = serde_json::to_value(payload).unwrap();
    assert_eq!(encoded["source_policy"], "FollowRegularFiles");
    assert!(encoded.get("original_binding").is_none());
}
