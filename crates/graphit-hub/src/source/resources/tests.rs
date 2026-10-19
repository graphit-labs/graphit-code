use super::*;
use crate::SourceRef;
use graphit_core::{brand::Brand, project_paths::InstallationPaths};
use std::fs;

fn limits() -> ProjectionLimits {
    ProjectionLimits::new(4096, 32 * 1024, 128, 8192, 128).unwrap()
}
fn fixture() -> (tempfile::TempDir, ProjectPaths) {
    let temp = tempfile::tempdir_in(fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
    let installation = InstallationPaths::resolve(
        Brand::new("resources-test").unwrap(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )
    .unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let project = ProjectPaths::at(&root, &installation).unwrap();
    (temp, project)
}
fn seed(project: &ProjectPaths, kind: ArtifactType, id: &str, version: &str) -> SourceRef {
    let source = SourceRef::resources(
        project,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        kind,
        ArtifactId::new(id, limits()).unwrap(),
        ConcreteVersion::new(version, limits()).unwrap(),
        limits(),
    )
    .unwrap();
    fs::create_dir_all(source.directory().as_path()).unwrap();
    source
}
fn query(limit: usize) -> ResourcesQuery<'static> {
    ResourcesQuery {
        artifact_type: None,
        id: None,
        version: None,
        after: None,
        limit: NonZeroUsize::new(limit).unwrap(),
    }
}
fn read(project: &ProjectPaths, query: ResourcesQuery<'_>) -> ResourcesPage {
    list_resources(project, query, limits(), &mut || Ok(())).unwrap()
}

#[test]
fn pages_are_lexical_exact_and_decode_utf8_across_chunk_boundaries() {
    let (_temp, project) = fixture();
    let long = format!("{}{}", "x".repeat(63), "é".repeat(50));
    let version = format!("{}é/release", "v".repeat(63));
    let fixtures = [
        (ArtifactType::Skill, "z", "2"),
        (ArtifactType::Skill, long.as_str(), version.as_str()),
        (ArtifactType::Command, "tools/a", "release/β"),
        (ArtifactType::Skill, "a", "2"),
        (ArtifactType::Skill, "a", "1"),
        (ArtifactType::Skill, "shared", "1"),
    ];
    let mut expected = Vec::new();
    for (kind, id, version) in fixtures {
        let source = seed(&project, kind, id, version);
        // Payload contents and names do not participate in identity discovery.
        fs::write(
            source.directory().as_path().join("not-an-identity"),
            [0xff, 0],
        )
        .unwrap();
        expected.push(ResourceIdentity {
            artifact_type: kind,
            id: source.id().clone(),
            version: source.version().clone(),
        });
    }
    expected.sort();
    let mut actual = Vec::new();
    let mut after = None;
    loop {
        let page = read(
            &project,
            ResourcesQuery {
                after: after.as_ref(),
                ..query(2)
            },
        );
        assert!(page.entries.len() <= 2);
        actual.extend(page.entries);
        after = page.next_after;
        if after.is_none() {
            break;
        }
    }
    assert_eq!(actual, expected);
    let selected_id = ArtifactId::new("a", limits()).unwrap();
    let selected_version = ConcreteVersion::new("2", limits()).unwrap();
    let page = read(
        &project,
        ResourcesQuery {
            artifact_type: Some(ArtifactType::Skill),
            id: Some(&selected_id),
            version: Some(&selected_version),
            ..query(2)
        },
    );
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].id.as_str(), "a");
    assert_eq!(page.entries[0].version.as_str(), "2");
    assert!(page.next_after.is_none());
}

#[test]
fn scan_reaches_eof_and_does_not_hide_malformed_namespace_after_a_full_page() {
    let (_temp, project) = fixture();
    seed(&project, ArtifactType::Skill, "a", "1");
    seed(&project, ArtifactType::Skill, "b", "1");
    let invalid = project
        .root()
        .join(project.brand().directory())
        .join("artifacts/power/ids/i-zz");
    fs::create_dir_all(&invalid).unwrap();
    let error = list_resources(&project, query(1), limits(), &mut || Ok(())).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    // An unrelated malformed kind does not alter an explicitly filtered read.
    assert_eq!(
        read(
            &project,
            ResourcesQuery {
                artifact_type: Some(ArtifactType::Skill),
                ..query(2)
            }
        )
        .entries
        .len(),
        2
    );
}

#[test]
fn noncanonical_chunks_invalid_utf8_and_concrete_identity_limits_are_rejected() {
    let (_temp, project) = fixture();
    let ids = project
        .root()
        .join(project.brand().directory())
        .join("artifacts/skill/ids");
    fs::create_dir_all(ids.join("i-61/i-62/versions/v-31/payload")).unwrap();
    assert_eq!(
        list_resources(&project, query(2), limits(), &mut || Ok(()))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    fs::remove_dir_all(&ids).unwrap();
    fs::create_dir_all(ids.join("i-ff/versions/v-31/payload")).unwrap();
    assert_eq!(
        list_resources(&project, query(2), limits(), &mut || Ok(()))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    fs::remove_dir_all(&ids).unwrap();
    seed(&project, ArtifactType::Skill, &"x".repeat(100), "1");
    let small = ProjectionLimits::new(64, 32 * 1024, 2, 8192, 128).unwrap();
    assert_eq!(
        list_resources(&project, query(2), small, &mut || Ok(()))
            .unwrap_err()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    assert_eq!(
        list_resources(&project, query(3), small, &mut || Ok(()))
            .unwrap_err()
            .stage(),
        ProjectionStage::Validate
    );
}

#[test]
fn absent_resources_are_empty_but_denial_and_cancellation_are_errors() {
    let (_temp, project) = fixture();
    assert!(read(&project, query(2)).entries.is_empty());
    let denied = list_resources(&project, query(2), limits(), &mut || {
        Err(io::ErrorKind::PermissionDenied.into())
    })
    .unwrap_err();
    assert_eq!(denied.kind(), io::ErrorKind::PermissionDenied);
    seed(&project, ArtifactType::Skill, "a", "1");
    let mut checks = 0;
    let error = list_resources(&project, query(2), limits(), &mut || {
        checks += 1;
        if checks > 20 {
            Err(io::ErrorKind::Interrupted.into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
}

#[cfg(unix)]
#[test]
fn payload_symlink_is_not_a_resources_identity() {
    let (temp, project) = fixture();
    let source = seed(&project, ArtifactType::Skill, "a", "1");
    fs::remove_dir(source.directory().as_path()).unwrap();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, source.directory().as_path()).unwrap();
    assert_eq!(
        list_resources(&project, query(2), limits(), &mut || Ok(()))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn payload_file_is_not_a_resources_directory() {
    let (_temp, project) = fixture();
    let source = seed(&project, ArtifactType::Skill, "a", "1");
    fs::remove_dir(source.directory().as_path()).unwrap();
    fs::write(source.directory().as_path(), "not a directory").unwrap();
    assert_eq!(
        list_resources(&project, query(2), limits(), &mut || Ok(()))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
}
