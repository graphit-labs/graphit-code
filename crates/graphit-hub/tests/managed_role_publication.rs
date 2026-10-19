use graphit_core::{
    artifact_store::{
        ArtifactChange, ArtifactLimits, ArtifactStore, CapturedPublicationScope, CopyLimits,
        ScopeLimits,
    },
    brand::Brand,
    file_lock::Cancellation,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_hub::{
    native::role_publication::{RoleDestination, RolePublicationError, publish_role},
    native::{ContentLimits, NativeBinding, Role, RoleCredentialReference, RoleSource, ToolGrants},
    policy::RequestContext,
};
use std::{
    fs, io,
    time::{Duration, Instant},
};
fn content() -> ContentLimits {
    ContentLimits {
        max_input_bytes: 65536,
        max_output_bytes: 65536,
        max_depth: 16,
        max_nodes: 1024,
        max_scalar_bytes: 8192,
        max_collection_entries: 64,
    }
}
fn file_limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 16384, 128, 8192).unwrap()
}
fn rendered(body: &str, req: &mut RequestContext<'_>) -> graphit_hub::native::RenderedContent {
    let descriptor = req
        .managed_agent("graphit-scout", "Inspect", body, true, false)
        .unwrap();
    req.render_agent(
        NativeBinding::Codex,
        &descriptor,
        None,
        &ToolGrants {
            binding: NativeBinding::Codex,
            read_only: true,
            allows_shell: false,
            tools: vec![],
        },
    )
    .unwrap()
}
fn fixture(req: &RequestContext<'_>) -> (tempfile::TempDir, ProjectPaths, ArtifactStore) {
    let temp = tempfile::tempdir().unwrap();
    let installation = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )
    .unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join(installation.brand().directory())).unwrap();
    let paths = ProjectPaths::at(root, &installation).unwrap();
    let identity = ProjectIdentityStore::at(&paths)
        .writer(Duration::from_secs(5), req.cancellation())
        .unwrap()
        .ensure(&ProjectMetadata::new("roles").unwrap())
        .unwrap();
    let scope = CapturedPublicationScope::project(
        &paths,
        identity.id(),
        ScopeLimits::new(1, 16384, 1024).unwrap(),
        req.snapshot(),
    )
    .unwrap();
    let copy = CopyLimits::new(1048576, 128, 65536, 16).unwrap();
    let store = ArtifactStore::new(scope, ArtifactLimits::new(1048576, 8192, copy).unwrap());
    (temp, paths, store)
}
fn reference() -> RoleCredentialReference {
    RoleCredentialReference::new(
        Role::Scout,
        RoleSource::ToolOwned(Brand::default()),
        [1; 32],
    )
}
#[test]
fn owned_role_updates_and_reopens_without_touching_foreign_agents() {
    let cancel = Cancellation::default();
    let mut req =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let (_temp, paths, store) = fixture(&req);
    let first = rendered("first", &mut req);
    assert_eq!(
        publish_role(
            &store,
            RoleDestination::Project(paths.root()),
            first,
            &reference(),
            file_limits(),
            &req,
            || req.remaining().map(|_| ())
        )
        .unwrap(),
        ArtifactChange::Created
    );
    let agent = paths.root().join(".codex/agents/graphit-scout.toml");
    let foreign = paths.root().join(".codex/agents/foreign.toml");
    fs::write(&foreign, "foreign").unwrap();
    let same = rendered("first", &mut req);
    assert_eq!(
        publish_role(
            &store,
            RoleDestination::Project(paths.root()),
            same,
            &reference(),
            file_limits(),
            &req,
            || Ok(())
        )
        .unwrap(),
        ArtifactChange::Unchanged
    );
    let second = rendered("second", &mut req);
    assert_eq!(
        publish_role(
            &store,
            RoleDestination::Project(paths.root()),
            second,
            &reference(),
            file_limits(),
            &req,
            || Ok(())
        )
        .unwrap(),
        ArtifactChange::Updated
    );
    assert!(fs::read_to_string(agent).unwrap().contains("second"));
    assert_eq!(fs::read_to_string(foreign).unwrap(), "foreign");
    assert_eq!(store.load().unwrap().unwrap().entries().len(), 1);
}
#[test]
fn denied_original_and_foreign_file_are_preserved() {
    let cancel = Cancellation::default();
    let mut req =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let (_temp, paths, store) = fixture(&req);
    let output = rendered("first", &mut req);
    assert!(
        matches!(publish_role(&store,RoleDestination::Project(paths.root()),output,&reference(),file_limits(),&req,||Err(io::ErrorKind::PermissionDenied.into())),Err(RolePublicationError::Input(e)) if e.kind()==io::ErrorKind::PermissionDenied)
    );
    assert!(!paths.root().join(".codex").exists());
    fs::create_dir_all(paths.root().join(".codex/agents")).unwrap();
    let file = paths.root().join(".codex/agents/graphit-scout.toml");
    fs::write(&file, "foreign").unwrap();
    let output = rendered("first", &mut req);
    assert!(matches!(
        publish_role(
            &store,
            RoleDestination::Project(paths.root()),
            output,
            &reference(),
            file_limits(),
            &req,
            || Ok(())
        ),
        Err(RolePublicationError::Physical(_))
    ));
    assert_eq!(fs::read_to_string(&file).unwrap(), "foreign");
    cancel.cancel();
    let output = graphit_hub::native::RenderedContent {
        relative_entry: ".codex/agents/graphit-scout.toml".into(),
        bytes: vec![],
        provenance: graphit_hub::native::ContentProvenance {
            renderer_schema: "codex-agent-toml",
            binding: NativeBinding::Codex,
            effective_name: "graphit-scout".into(),
            input_fingerprint: [0; 32],
            output_fingerprint: [0; 32],
        },
    };
    assert!(
        publish_role(
            &store,
            RoleDestination::Project(paths.root()),
            output,
            &reference(),
            file_limits(),
            &req,
            || Ok(())
        )
        .is_err()
    );
    assert_eq!(fs::read_to_string(file).unwrap(), "foreign");
}

#[test]
fn generated_opencode_plugin_updates_by_hash_and_preserves_user_edits() {
    use graphit_hub::native::opencode_plugin::{
        OpenCodePluginInput, publish_opencode_plugin, remove_opencode_plugin,
        render_opencode_plugin,
    };
    let cancel = Cancellation::default();
    let req =
        RequestContext::new(content(), &cancel, Instant::now() + Duration::from_secs(30)).unwrap();
    let (_temp, paths, store) = fixture(&req);
    let mut context = std::process::Command::new("graphit");
    context.args(["hook", "context"]);
    let mut compaction = std::process::Command::new("graphit");
    compaction.args(["hook", "compaction"]);
    let plan = render_opencode_plugin(
        OpenCodePluginInput {
            identifier: "graphit",
            context: &context,
            compaction: &compaction,
            idle: None,
            deleted: None,
            timeout: Duration::from_secs(5),
            max_input_bytes: 8192,
            max_output_bytes: 8192,
        },
        &req,
    )
    .unwrap();
    let publish =
        || publish_opencode_plugin(&store, paths.root(), &plan, file_limits(), &req, || Ok(()));
    assert_eq!(publish().unwrap(), ArtifactChange::Created);
    let destination = paths.root().join(plan.relative_entry());
    assert_eq!(fs::read(&destination).unwrap(), plan.bytes());
    assert_eq!(publish().unwrap(), ArtifactChange::Unchanged);
    assert!(!store.runtime_root().join("owned-payloads").exists());
    fs::write(&destination, b"// user maintained change\n").unwrap();
    assert!(publish().is_err());
    assert!(remove_opencode_plugin(&store, "graphit", &req, || Ok(())).is_err());
    assert_eq!(
        fs::read(&destination).unwrap(),
        b"// user maintained change\n"
    );
}
