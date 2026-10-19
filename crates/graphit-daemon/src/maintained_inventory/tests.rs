use super::*;
use crate::auth_startup::StartupPolicy;
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ServiceAdmissionLimits,
    ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy, profile::ProfileLimits,
};
use graphit_core::{
    artifact_store::ScopeLimits,
    auth_document::AuthDocumentLimits,
    brand::Brand,
    file_lock::Cancellation,
    git_checkout::GitCheckout,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationIdentityStore,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_inventory::CapturedProjectInventory,
    project_paths::ProjectPaths,
    runtime_key::{RuntimeKeyEffect, RuntimeKeyLimits},
    version_context::{Checkout, VersionContext},
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}
fn startup_request(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        cancel,
        deadline,
    )
    .unwrap()
}
fn scopes() -> ScopeLimits {
    ScopeLimits::new(16, 4096, 128).unwrap()
}
fn documents() -> AuthDocumentLimits {
    AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn profiles() -> ProfileLimits {
    ProfileLimits {
        bytes: 65536,
        nodes: 4096,
        scalar_bytes: 4096,
        collection_entries: 64,
        providers: 16,
        profiles: 16,
    }
}
fn policy() -> StartupPolicy {
    let permissions = [Permission::Read(Module::Ast)];
    let source = || SourceAuthorityPolicy {
        ordinary: &permissions,
        administrative: &[],
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    StartupPolicy {
        bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
        runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
        scope: scopes(),
        document: documents(),
        profiles: profiles(),
        authority: IssuerAuthorityPolicy::new(
            source(),
            source(),
            IssuerLimits {
                max_permissions: 64,
                max_scope_bytes: 65536,
                max_parent_bearer_bytes: 1024,
            },
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 8192,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap(),
        ttl: TtlPolicy {
            ttl: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
            refresh_margin: Duration::from_secs(120),
        },
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    paths: InstallationPaths,
    auth: DaemonAuth,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let cancellation = Cancellation::default();
        let request = startup_request(&cancellation, Instant::now() + Duration::from_secs(30));
        let project_root = directory.path().join("trusted-bootstrap-project");
        fs::create_dir(&project_root).unwrap();
        let project = ProjectPaths::at(&project_root, &paths).unwrap();
        let auth =
            DaemonAuth::start(&paths, std::slice::from_ref(&project), policy(), &request).unwrap();
        cancellation.cancel();
        Self {
            directory,
            paths,
            auth,
        }
    }
    fn document(&self) -> PathBuf {
        self.paths.root().join("project-inventory.json")
    }
    fn read(&self) -> MaintainedInventorySnapshot {
        read_maintained_inventory(&self.auth, &original(), 65536).unwrap()
    }
    fn project(&self, name: &str) -> ProjectPaths {
        let root = self.directory.path().join(name);
        fs::create_dir(&root).unwrap();
        let paths = ProjectPaths::at(root, &self.paths).unwrap();
        ProjectIdentityStore::at(&paths)
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
            .ensure(&ProjectMetadata::new(name).unwrap())
            .unwrap();
        paths
    }
    fn register(&self, project: &ProjectPaths, selected: &str, retained: &[&str]) -> ProjectId {
        let req = original();
        let version =
            VersionContext::select(project, Checkout::NonGit, selected, None, "").unwrap();
        let cap = CapturedProjectInventory::capture(
            project,
            &GitCheckout::NonGit,
            &version,
            scopes(),
            NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
            &req,
        )
        .unwrap();
        let private = installation_bootstrap::prepare(
            &self.paths,
            BootstrapLimits::new(65536, 256).unwrap(),
            &req,
        )
        .into_installation()
        .unwrap();
        let store = GlobalProjectInventory::at(&self.paths, 65536).unwrap();
        let mut writer = store.writer(private, &req).unwrap();
        writer.register_checkout(&cap).unwrap();
        for version in retained {
            writer.retain_version(&cap, version).unwrap();
        }
        ProjectIdentityStore::at(project)
            .load()
            .unwrap()
            .unwrap()
            .id()
    }
    fn write(&self, bytes: &[u8]) {
        fs::write(self.document(), bytes).unwrap();
        fs::set_permissions(self.document(), fs::Permissions::from_mode(0o600)).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let result = self.auth.remove_runtime(&original());
        assert_eq!(result.effects().key, RuntimeKeyEffect::Applied);
        assert!(result.failure().is_none());
        assert!(result.cleanup_failure().is_none());
    }
}

#[test]
fn missing_inventory_is_readonly_and_startup_cancellation_is_not_retained() {
    let f = Fixture::new();
    let before = fs::read_dir(f.paths.root())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect::<std::collections::BTreeSet<_>>();
    let id = InstallationIdentityStore::new(&f.paths)
        .unwrap()
        .load()
        .unwrap()
        .unit_id()
        .unwrap()
        .clone();
    let snapshot = f.read();
    assert_eq!(snapshot.installation_id, id);
    assert!(snapshot.projects.is_empty());
    assert_eq!(f.read(), snapshot);
    let config = f.paths.config_file();
    let parked_config = f.directory.path().join("parked-config.json");
    fs::rename(&config, &parked_config).unwrap();
    assert_eq!(f.read(), snapshot);
    assert!(!config.exists());
    fs::rename(parked_config, config).unwrap();
    assert!(!f.document().exists());
    assert!(!f.paths.root().join(".project-inventory.lock").exists());
    assert_eq!(
        fs::read_dir(f.paths.root())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<std::collections::BTreeSet<_>>(),
        before
    );
    f.write(
        &serde_json::to_vec(&serde_json::json!({
            "schema": 1, "brand": f.paths.brand().key(), "projects": []
        }))
        .unwrap(),
    );
    assert_eq!(f.read(), snapshot);
}

#[test]
fn captured_missing_checkout_does_not_invalidate_historical_installation_inventory() {
    let f = Fixture::new();
    let first = ProjectPaths::at(
        f.directory.path().join("trusted-bootstrap-project"),
        &f.paths,
    )
    .unwrap();
    ProjectIdentityStore::at(&first)
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .ensure(&ProjectMetadata::new("historical").unwrap())
        .unwrap();
    let first_id = f.register(&first, "", &["old/β"]);
    let second = f.project("still-present");
    f.register(&second, "topic", &[]);
    let before = f.read();
    fs::remove_dir_all(first.root()).unwrap();
    let context = ScopeContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    assert!(
        f.auth
            .installation()
            .live
            .revalidate_bounded(&context)
            .is_err()
    );
    assert_eq!(f.read(), before);
    assert!(
        f.auth
            .installation()
            .live
            .revalidate_bounded(&context)
            .is_err()
    );
    let req = original();
    let private =
        installation_bootstrap::prepare(&f.paths, BootstrapLimits::new(65536, 256).unwrap(), &req)
            .into_installation()
            .unwrap();
    let store = GlobalProjectInventory::at(&f.paths, 65536).unwrap();
    assert!(
        store
            .writer(private, &req)
            .unwrap()
            .remove_checkout(first_id, first.root())
            .unwrap()
    );
    let metadata = store.load(&req).unwrap();
    let entry = metadata.get(first_id).unwrap();
    assert_eq!(entry.checkouts().len(), 0);
    assert_eq!(entry.versions().collect::<Vec<_>>(), ["main", "old/β"]);
    assert_eq!(f.read(), before);
    assert!(!first.root().exists());
    assert!(second.root().exists());
}

#[test]
fn actual_writers_reload_isolate_projects_and_keep_old_versions_without_opening_checkouts() {
    let a = Fixture::new();
    let b = Fixture::new();
    let first = a.project("first");
    let second = a.project("second");
    let foreign = b.project("foreign");
    let first_id = a.register(&first, "", &["old/β"]);
    let second_id = a.register(&second, "topic", &[]);
    let foreign_id = b.register(&foreign, "foreign-only", &[]);
    let before = a.read();
    assert_eq!(before.projects.len(), 2);
    assert_ne!(before.installation_id, b.read().installation_id);
    assert!(!before.projects.iter().any(|p| p.project_id == foreign_id));
    assert_eq!(b.read().projects[0].versions, ["foreign-only"]);
    assert_eq!(
        before
            .projects
            .iter()
            .find(|p| p.project_id == first_id)
            .unwrap()
            .versions,
        ["main", "old/β"]
    );
    assert_eq!(
        before
            .projects
            .iter()
            .find(|p| p.project_id == second_id)
            .unwrap()
            .versions,
        ["topic"]
    );
    a.register(&first, "next", &["new-data"]);
    let updated = a.read();
    assert_eq!(
        updated
            .projects
            .iter()
            .find(|p| p.project_id == first_id)
            .unwrap()
            .versions,
        ["main", "new-data", "next", "old/β"]
    );
    let bytes = fs::read(a.document()).unwrap();
    fs::remove_dir_all(first.root()).unwrap();
    fs::remove_dir_all(second.root()).unwrap();
    assert_eq!(a.read(), updated);
    assert_eq!(fs::read(a.document()).unwrap(), bytes);
    assert_eq!(
        updated
            .projects
            .iter()
            .map(|p| p.project_id)
            .collect::<Vec<_>>(),
        GlobalProjectInventory::at(&a.paths, 65536)
            .unwrap()
            .load(&original())
            .unwrap()
            .projects()
            .map(|p| p.identity().id())
            .collect::<Vec<_>>()
    );
}

#[test]
fn cancelled_and_expired_reads_preserve_the_document() {
    let f = Fixture::new();
    f.register(&f.project("checkout"), "", &[]);
    let bytes = fs::read(f.document()).unwrap();
    let cancelled = original();
    cancelled.cancellation().cancel();
    assert_eq!(
        read_maintained_inventory(&f.auth, &cancelled, 65536)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    assert_eq!(
        read_maintained_inventory(&f.auth, &expired, 65536)
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
}

#[test]
fn startup_expiry_does_not_expire_fresh_reads() {
    let directory = tempfile::tempdir().unwrap();
    let paths = InstallationPaths::resolve(
        Brand::default(),
        directory.path(),
        None,
        Some(&directory.path().join("installation")),
    )
    .unwrap();
    let cancellation = Cancellation::default();
    let request = startup_request(&cancellation, Instant::now() + Duration::from_secs(1));
    let project_root = directory.path().join("trusted-bootstrap-project");
    fs::create_dir(&project_root).unwrap();
    let project = ProjectPaths::at(&project_root, &paths).unwrap();
    let auth =
        DaemonAuth::start(&paths, std::slice::from_ref(&project), policy(), &request).unwrap();
    std::thread::sleep(
        request
            .snapshot()
            .deadline()
            .expect("finite fixture deadline")
            .saturating_duration_since(Instant::now())
            + Duration::from_millis(1),
    );
    assert_eq!(
        request.snapshot().check().unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(
        read_maintained_inventory(&auth, &original(), 65536)
            .unwrap()
            .projects
            .is_empty()
    );
    let removed = auth.remove_runtime(&original());
    assert_eq!(removed.effects().key, RuntimeKeyEffect::Applied);
    assert!(removed.failure().is_none());
    assert!(removed.cleanup_failure().is_none());
}

#[test]
fn moved_and_replaced_live_namespace_is_an_error_even_without_inventory() {
    let f = Fixture::new();
    let moved = f.directory.path().join("moved");
    fs::rename(f.paths.root(), &moved).unwrap();
    assert!(read_maintained_inventory(&f.auth, &original(), 65536).is_err());
    fs::create_dir(f.paths.root()).unwrap();
    fs::set_permissions(f.paths.root(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(read_maintained_inventory(&f.auth, &original(), 65536).is_err());
    fs::remove_dir(f.paths.root()).unwrap();
    fs::rename(moved, f.paths.root()).unwrap();
    assert!(f.read().projects.is_empty());
}

#[test]
fn invalid_and_too_small_bounds_keep_real_core_errors() {
    let f = Fixture::new();
    for cap in [0, usize::MAX] {
        assert_eq!(
            read_maintained_inventory(&f.auth, &original(), cap)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    f.register(&f.project("checkout"), "", &[]);
    let bytes = fs::read(f.document()).unwrap();
    assert_eq!(
        read_maintained_inventory(&f.auth, &original(), bytes.len() - 1)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(f.document()).unwrap(), bytes);
}

#[test]
fn malformed_schema_type_and_duplicate_documents_are_not_empty_fallbacks() {
    let f = Fixture::new();
    f.register(&f.project("checkout"), "", &[]);
    let bytes = fs::read(f.document()).unwrap();
    let baseline: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut cases = vec![
        b"{".to_vec(),
        br#"{"schema":1,"schema":1,"brand":"graphit","projects":[]}"#.to_vec(),
    ];
    for (field, value) in [
        ("schema", serde_json::json!(2)),
        ("projects", serde_json::json!(0)),
        ("brand", serde_json::json!("other")),
        ("extra", serde_json::json!(true)),
    ] {
        let mut changed = baseline.clone();
        changed[field] = value;
        cases.push(serde_json::to_vec(&changed).unwrap());
    }
    let mut duplicate = baseline;
    let entry = duplicate["projects"][0].clone();
    duplicate["projects"].as_array_mut().unwrap().push(entry);
    cases.push(serde_json::to_vec(&duplicate).unwrap());
    for case in cases {
        f.write(&case);
        let core = GlobalProjectInventory::at(&f.paths, 65536)
            .unwrap()
            .load(&original())
            .unwrap_err()
            .kind();
        assert_eq!(core, io::ErrorKind::InvalidData);
        assert_eq!(
            read_maintained_inventory(&f.auth, &original(), 65536)
                .unwrap_err()
                .kind(),
            core
        );
        assert_eq!(fs::read(f.document()).unwrap(), case);
    }
}

#[test]
fn private_permissions_links_and_directory_errors_match_the_core_reader() {
    let f = Fixture::new();
    f.register(&f.project("checkout"), "", &[]);
    let bytes = fs::read(f.document()).unwrap();
    fs::set_permissions(f.document(), fs::Permissions::from_mode(0o644)).unwrap();
    let core = GlobalProjectInventory::at(&f.paths, 65536)
        .unwrap()
        .load(&original())
        .unwrap_err()
        .kind();
    assert_eq!(core, io::ErrorKind::PermissionDenied);
    assert_eq!(
        read_maintained_inventory(&f.auth, &original(), 65536)
            .unwrap_err()
            .kind(),
        core
    );
    fs::remove_file(f.document()).unwrap();
    let target = f.directory.path().join("target.json");
    fs::write(&target, &bytes).unwrap();
    symlink(&target, f.document()).unwrap();
    let core = GlobalProjectInventory::at(&f.paths, 65536)
        .unwrap()
        .load(&original())
        .unwrap_err()
        .kind();
    assert_eq!(
        read_maintained_inventory(&f.auth, &original(), 65536)
            .unwrap_err()
            .kind(),
        core
    );
    fs::remove_file(f.document()).unwrap();
    fs::create_dir(f.document()).unwrap();
    let core = GlobalProjectInventory::at(&f.paths, 65536)
        .unwrap()
        .load(&original())
        .unwrap_err()
        .kind();
    assert_eq!(
        read_maintained_inventory(&f.auth, &original(), 65536)
            .unwrap_err()
            .kind(),
        core
    );
    fs::remove_dir(f.document()).unwrap();
    f.write(&bytes);
    assert_eq!(f.read().projects.len(), 1);
}
