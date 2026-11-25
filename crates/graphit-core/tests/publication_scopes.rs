//! Public artifact consumers use logical scopes and ordinary configured paths.
use graphit_core::{
    artifact_store::{
        ArtifactKey, ArtifactLimits, ArtifactMode, ArtifactStore, CopyLimits, CopyPolicy,
        PayloadSelection, PublicationScope,
    },
    brand::Brand,
    file_lock::Cancellation,
    installation_identity::InstallationId,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_fs::NodeKind;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
fn limits() -> ArtifactLimits {
    ArtifactLimits::new(
        1024 * 1024,
        1024,
        CopyLimits::new(1024 * 1024, 100).unwrap(),
    )
    .unwrap()
}
fn installation(root: &Path) -> InstallationPaths {
    InstallationPaths::resolve(Brand::default(), root, None, Some(&root.join("global"))).unwrap()
}
fn key() -> ArtifactKey {
    ArtifactKey::new("native", "skill", "guide").unwrap()
}
#[test]
fn worktrees_share_a_catalogue_but_update_and_remove_only_the_selected_installation() {
    let root = tempfile::tempdir().unwrap();
    let installation = installation(root.path());
    let source = root.path().join("source");
    fs::write(&source, b"package").unwrap();
    let mut stores = Vec::new();
    for name in ["first", "second"] {
        let path = root.path().join(name);
        fs::create_dir(&path).unwrap();
        let project = ProjectPaths::at(&path, &installation).unwrap();
        let store = ArtifactStore::new(
            PublicationScope::project(&project, "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap())
                .unwrap(),
            limits(),
        );
        let dest = project.root().join("skills/guide");
        let mut writer = store
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap();
        writer
            .publish(
                &key(),
                &source,
                &dest,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::FollowLinks),
            )
            .unwrap();
        stores.push((store, dest));
    }
    assert_eq!(stores[0].0.runtime_root(), stores[1].0.runtime_root());
    assert_eq!(stores[0].0.load().unwrap().unwrap().entries().len(), 2);
    fs::write(&source, b"upgrade").unwrap();
    stores[0]
        .0
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .publish(
            &key(),
            &source,
            &stores[0].1,
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::FollowLinks),
        )
        .unwrap();
    assert_eq!(fs::read(&stores[0].1).unwrap(), b"upgrade");
    assert_eq!(fs::read(&stores[1].1).unwrap(), b"package");
    stores[0]
        .0
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap()
        .remove(&key())
        .unwrap();
    assert_eq!(fs::read(&stores[1].1).unwrap(), b"package");
    assert_eq!(stores[1].0.load().unwrap().unwrap().entries().len(), 1);
}
#[test]
fn installation_targets_stay_inside_authorized_roots_without_capturing_them() {
    let root = tempfile::tempdir().unwrap();
    let installation = installation(root.path());
    let allowed = root.path().join("agents");
    let scope = PublicationScope::installation(
        &installation,
        InstallationId::new("fixture").unwrap(),
        [allowed.clone()],
    )
    .unwrap();
    let store = ArtifactStore::new(scope, limits());
    let source = root.path().join("package");
    fs::write(&source, b"source").unwrap();
    let mut writer = store
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap();
    for dest in [
        allowed.clone(),
        allowed.join("../outside"),
        root.path().join("other"),
        store.runtime_root().join("bad"),
    ] {
        assert!(
            writer
                .publish(
                    &key(),
                    &source,
                    &dest,
                    NodeKind::File,
                    ArtifactMode::Copy(CopyPolicy::FollowLinks)
                )
                .is_err()
        );
        assert!(!dest.is_file());
    }
    let dest = allowed.join("nested/ação.md");
    writer
        .publish(
            &key(),
            &source,
            &dest,
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::FollowLinks),
        )
        .unwrap();
    assert_eq!(fs::read(dest).unwrap(), b"source");
}
#[test]
fn decoding_external_payload_selection_rejects_escape_and_invalid_shapes() {
    for relative in ["../escape", "a/../../escape", "/absolute"] {
        let input = serde_json::json!({"relative":relative,"kind":"File"});
        assert!(serde_json::from_value::<PayloadSelection>(input).is_err());
    }
    let value = PayloadSelection::new(PathBuf::from("docs/ação.md"), NodeKind::File).unwrap();
    let decoded: PayloadSelection =
        serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(decoded.relative(), Path::new("docs/ação.md"));
    assert_eq!(decoded.kind(), NodeKind::File);
}
#[cfg(unix)]
#[test]
fn configured_roots_can_be_symlinks_and_are_not_pinned_to_the_old_directory() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let installation = installation(root.path());
    let actual = root.path().join("actual");
    fs::create_dir(&actual).unwrap();
    let alias = root.path().join("agents");
    symlink(&actual, &alias).unwrap();
    let store = ArtifactStore::new(
        PublicationScope::installation(
            &installation,
            InstallationId::new("fixture").unwrap(),
            [alias.clone()],
        )
        .unwrap(),
        limits(),
    );
    let source = root.path().join("package");
    fs::write(&source, b"source").unwrap();
    let mut writer = store
        .writer(Duration::from_secs(5), &Cancellation::default())
        .unwrap();
    writer
        .publish(
            &key(),
            &source,
            &alias.join("guide"),
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::FollowLinks),
        )
        .unwrap();
    assert_eq!(fs::read(actual.join("guide")).unwrap(), b"source");
    let second = root.path().join("second");
    fs::create_dir(&second).unwrap();
    fs::remove_file(&alias).unwrap();
    symlink(&second, &alias).unwrap();
    writer
        .publish(
            &key(),
            &source,
            &alias.join("guide"),
            NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::FollowLinks),
        )
        .unwrap();
    assert_eq!(fs::read(second.join("guide")).unwrap(), b"source");
    assert_eq!(fs::read(actual.join("guide")).unwrap(), b"source");
}
