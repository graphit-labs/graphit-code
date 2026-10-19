//! Retained native contexts sharing the existing authenticated fixture and policies.
use super::*;

pub(crate) fn browser_workspace_fixture(
    content: Option<ContentLimits>,
    timeout: Duration,
) -> (tempfile::TempDir, TaskServer, String) {
    workspace_fixture(&[Permission::ManageAuthProfiles], content, timeout)
}
pub(crate) fn memory_workspace_fixture(
    content: Option<ContentLimits>,
    timeout: Duration,
) -> (tempfile::TempDir, TaskServer, String) {
    workspace_fixture(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Memory),
            Permission::WriteMemory,
        ],
        content,
        timeout,
    )
}
pub(crate) fn knowledge_workspace_fixture(
    content: Option<ContentLimits>,
    timeout: Duration,
) -> (tempfile::TempDir, TaskServer, String) {
    workspace_fixture(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
            Permission::Read(Module::Memory),
            Permission::WriteMemory,
        ],
        content,
        timeout,
    )
}
fn workspace_fixture(
    extra: &[Permission],
    content: Option<ContentLimits>,
    timeout: Duration,
) -> (tempfile::TempDir, TaskServer, String) {
    let (temp, original, raw) = fixture_with_root_names(extra, &["other", "mirror"]);
    let installation = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )
    .unwrap();
    let snapshot = original
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let first = &snapshot.projects[0];
    let mut projects = vec![first.clone()];
    let cancel = Cancellation::default();
    for (name, shared_identity) in [("other", false), ("mirror", true)] {
        let root = temp.path().join(name);
        assert!(root.is_dir());
        let paths = ProjectPaths::at(&root, &installation).unwrap();
        if shared_identity {
            std::fs::create_dir_all(paths.brand_directory()).unwrap();
            std::fs::copy(first.0.identity_file(), paths.identity_file()).unwrap();
        }
        let id = ProjectIdentityStore::at(&paths)
            .writer(Duration::from_secs(5), &cancel)
            .unwrap()
            .ensure(&ProjectMetadata::new(name).unwrap())
            .unwrap()
            .id();
        let checkout = GitCheckout::inspect(&root, Duration::from_secs(5), &cancel).unwrap();
        let version = VersionContext::select(&paths, checkout.as_checkout(), "", None, "").unwrap();
        projects.push((paths, id, version));
    }
    let mut policy = original.0.policy.clone();
    if let Some(content) = content {
        policy.content = content;
    }
    policy.timeout = timeout;
    let server = TaskServer::new(
        Arc::clone(&original.0.auth),
        Arc::new(projects),
        snapshot.current.expect("active fixture"),
        policy,
        CancellationToken::new(),
    )
    .unwrap();
    (temp, server, raw)
}
