use graphit_core::{
    artifact_store::{
        CapturedSourceReader, ProjectIgnoreCriteria, ProjectSelectionCriteria,
        ProjectSelectionLimits, PublicationScope, SourceReadLimits,
    },
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    project_paths::{InstallationPaths, ProjectPaths},
};
use std::{
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

/// Corpus and embeddings live outside the checkout; checkout state is excluded by policy.
#[test]
fn source_selection_excludes_checkout_state_and_global_corpus() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path().join("project");
    let global_root = temp.path().join("global");
    fs::create_dir(&project_root).unwrap();
    let installation =
        InstallationPaths::resolve(Brand::default(), temp.path(), None, Some(&global_root))
            .unwrap();
    let project = ProjectPaths::at(&project_root, &installation).unwrap();
    let scope =
        PublicationScope::project(&project, "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap()).unwrap();
    fs::create_dir(project_root.join("docs")).unwrap();
    fs::write(project_root.join("docs/a.md"), b"document\n").unwrap();
    fs::create_dir_all(project.brand_directory()).unwrap();
    fs::write(
        project.brand_directory().join("private.md"),
        b"checkout state",
    )
    .unwrap();
    let context = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ));
    let capture = || {
        let protected = format!("{}/", project.brand().directory());
        CapturedSourceReader::capture_selected_project(
            &scope,
            ProjectSelectionCriteria {
                subtree: Some(Path::new(".")),
                extras: &[],
                include_readme: true,
                extensions: &[".md"],
                ignore: ProjectIgnoreCriteria {
                    protected_defaults: &[&protected],
                    custom_control: None,
                    git_control: None,
                },
            },
            ProjectSelectionLimits {
                max_input_items: 64,
                max_input_bytes: 4096,
                max_selected_file_bytes: 65536,
                max_control_file_bytes: 4096,
            },
            SourceReadLimits::new(65536, 4096, 131072).unwrap(),
            &context,
        )
        .unwrap()
    };
    let before = capture();
    for directory in ["knowledge/index.lance", "knowledge/embeddings.lance"] {
        let path = scope.runtime_root().join(directory);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("internal.md"), b"derived state").unwrap();
    }
    let after = capture();
    for source in [&before, &after] {
        let files: Vec<_> = source
            .nodes()
            .filter(|node| node.kind() == graphit_fs::NodeKind::File)
            .map(|node| node.relative().to_owned())
            .collect();
        assert_eq!(files, [Path::new("docs/a.md")]);
        assert_eq!(
            source.retained(Path::new("docs/a.md")),
            Some(&b"document\n"[..])
        );
    }
    assert_eq!(before.fingerprint(), after.fingerprint());
}
