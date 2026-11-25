use super::*;
use std::fs;

fn installation(root: &Path) -> InstallationPaths {
    InstallationPaths::resolve(Brand::default(), root, None, Some(&root.join("global"))).unwrap()
}

#[test]
fn project_selection_uses_logical_owner_and_distinguishes_worktrees() {
    let root = tempfile::tempdir().unwrap();
    let installation = installation(root.path());
    let id: ProjectId = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
    let mut scopes = Vec::new();
    for name in ["first", "second"] {
        let path = root.path().join(name);
        fs::create_dir(&path).unwrap();
        let project = ProjectPaths::at(&path, &installation).unwrap();
        scopes.push(PublicationScope::project(&project, id).unwrap());
    }
    assert_eq!(scopes[0].owner(), scopes[1].owner());
    assert_eq!(scopes[0].runtime_root(), scopes[1].runtime_root());
    assert_ne!(scopes[0].publication_slot(), scopes[1].publication_slot());
    for scope in scopes {
        let slot = scope.publication_slot();
        let restored: PublicationSlot =
            serde_json::from_slice(&serde_json::to_vec(&slot).unwrap()).unwrap();
        assert_eq!(restored, slot);
        restored.validate(scope.owner()).unwrap();
        assert!(!scope.runtime_root().exists());
    }
}

#[test]
fn configured_destinations_allow_missing_roots_without_creating_them() {
    let root = tempfile::tempdir().unwrap();
    let installation = installation(root.path());
    let destination = root.path().join("external");
    let scope = PublicationScope::installation(
        &installation,
        InstallationId::new("fixture").unwrap(),
        [destination.clone()],
    )
    .unwrap();
    scope
        .destination(&destination.join("package").join("README.md"))
        .unwrap();
    scope
        .destination(&destination.join("nested/ação.md"))
        .unwrap();
    for path in [
        destination.clone(),
        destination.join("..").join("outside"),
        root.path().join("external-other").join("file"),
        scope.runtime_root().join("catalogue"),
        PathBuf::from("relative"),
    ] {
        assert!(scope.destination(&path).is_err(), "{path:?}");
    }
    assert!(!destination.exists());
    assert!(!installation.root().exists());
}

#[test]
fn user_namespace_uses_exact_identity_and_contains_record_selection() {
    let root = tempfile::tempdir().unwrap();
    let installation = installation(root.path());
    let identity = UserIdentity::new("issuer:a".into(), "b".into()).unwrap();
    let other = UserIdentity::new("issuer".into(), "a:b".into()).unwrap();
    assert_ne!(identity.namespace_key(), other.namespace_key());
    let scope = PublicationScope::user(
        &installation,
        InstallationId::new("fixture").unwrap(),
        identity.clone(),
    )
    .unwrap();
    let expected = installation
        .root()
        .join("users")
        .join(identity.namespace_key());
    assert_eq!(scope.user_root().unwrap(), expected);
    assert_eq!(scope.runtime_root(), expected.join("runtime"));
    scope.destination(&expected.join("memory")).unwrap();
    assert!(
        scope
            .destination(
                &installation
                    .root()
                    .join("users")
                    .join(other.namespace_key())
                    .join("memory")
            )
            .is_err()
    );
    assert!(
        scope
            .destination(&expected.join("../other/memory"))
            .is_err()
    );
    assert_eq!(scope.publication_slot(), PublicationSlot::Scope);
    assert!(!installation.root().exists());
}

#[cfg(unix)]
#[test]
fn configured_linked_roots_follow_normal_filesystem_resolution() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let installation = installation(root.path());
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap();
    symlink(&target, installation.root()).unwrap();
    let destination = root.path().join("destination");
    symlink(&target, &destination).unwrap();
    let scope = PublicationScope::installation(
        &installation,
        InstallationId::new("fixture").unwrap(),
        [destination.clone()],
    )
    .unwrap();
    let path = destination.join("installed");
    scope.destination(&path).unwrap();
    fs::write(&path, b"published through configured link").unwrap();
    assert_eq!(
        fs::read(target.join("installed")).unwrap(),
        b"published through configured link"
    );
    // Replacing the configured link is ordinary path resolution, not a stale scope.
    let next = root.path().join("next");
    fs::create_dir(&next).unwrap();
    fs::remove_file(&destination).unwrap();
    symlink(&next, &destination).unwrap();
    scope.destination(&path).unwrap();
    fs::write(&path, b"next configured target").unwrap();
    assert_eq!(
        fs::read(next.join("installed")).unwrap(),
        b"next configured target"
    );
}
