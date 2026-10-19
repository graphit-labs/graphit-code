#![cfg(target_os = "linux")]

use graphit_core::{
    artifact_store::{
        ArtifactLimits, ArtifactStore, CapturedPublicationScope, CapturedSourceReader, CopyLimits,
        ProjectIgnoreCriteria, ProjectRecordDirectory, ProjectSelectionCriteria,
        ProjectSelectionLimits, ScopeLimits, SourceReadLimits,
    },
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
};
use std::{
    fs, io,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

struct Fixture {
    _temp: tempfile::TempDir,
    project: ProjectPaths,
    original: Arc<SnapshotContext>,
    scope: CapturedPublicationScope,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let installation_root = root.join("installation");
        let project_root = root.join("project");
        fs::create_dir(&installation_root).unwrap();
        fs::create_dir(&project_root).unwrap();
        fs::create_dir(project_root.join("docs")).unwrap();
        fs::write(project_root.join("docs/a.md"), b"document\n").unwrap();
        let installation =
            InstallationPaths::resolve(Brand::default(), &root, None, Some(&installation_root))
                .unwrap();
        let project = ProjectPaths::at(&project_root, &installation).unwrap();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        let scope = CapturedPublicationScope::project(
            &project,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            ScopeLimits::new(4, 16 * 1024, 128).unwrap(),
            &original,
        )
        .unwrap();
        Self {
            _temp: temp,
            project,
            original,
            scope,
        }
    }

    fn artifacts(&self) -> ArtifactStore {
        let copy = CopyLimits::new(1024 * 1024, 128, 16 * 1024, 16).unwrap();
        ArtifactStore::new(
            self.scope.clone(),
            ArtifactLimits::new(128 * 1024, 1024, copy).unwrap(),
        )
    }

    fn source(&self) -> CapturedSourceReader {
        let brand_ignore = format!("{}/", self.project.brand().directory());
        CapturedSourceReader::capture_selected_project(
            &self.scope,
            ProjectSelectionCriteria {
                subtree: Some(Path::new("docs")),
                extras: &[],
                include_readme: false,
                extensions: &[".md"],
                ignore: ProjectIgnoreCriteria {
                    protected_defaults: &[&brand_ignore],
                    custom_control: None,
                    git_control: None,
                },
            },
            ProjectSelectionLimits {
                max_depth: 16,
                max_input_items: 64,
                max_input_bytes: 4096,
                max_observed_path_bytes: 4096,
                max_selected_file_bytes: 65536,
                max_control_file_bytes: 4096,
            },
            SourceReadLimits::new(65536, 4096, 131072).unwrap(),
            &self.original,
        )
        .unwrap()
    }

    fn assert_document(&self, reader: &CapturedSourceReader) {
        reader.check_request(&self.original).unwrap();
        assert_eq!(
            reader.retained(Path::new("docs/a.md")),
            Some(&b"document\n"[..])
        );
        assert_eq!(
            fs::read(self.project.root().join("docs/a.md")).unwrap(),
            b"document\n"
        );
        assert!(
            !self
                .scope
                .runtime_root()
                .join("knowledge/index.lance")
                .exists()
        );
    }
}

fn record_limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(128 * 1024, 16 * 1024, 128, 16 * 1024).unwrap()
}

#[test]
fn brand_birth_after_capture_is_a_real_source_change() {
    let f = Fixture::new();
    assert!(!f.project.brand_directory().exists());
    let source = f.source();
    let writer = f.artifacts().writer_with_context(&f.original).unwrap();
    assert!(f.project.brand_directory().exists());
    assert_eq!(
        source.revalidate_content().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(!f.scope.runtime_root().join("knowledge").exists());
    f.assert_document(&source);
    drop(writer);
}

#[test]
fn publication_inside_existing_pruned_brand_preserves_source() {
    let f = Fixture::new();
    drop(graphit_fs::create_private_directory(&f.project.brand_directory()).unwrap());
    let source = f.source();
    let mut writer = f.artifacts().writer_with_context(&f.original).unwrap();
    let directory = writer
        .create_record_directory(ProjectRecordDirectory::Knowledge, record_limits())
        .unwrap();
    directory.revalidate(&f.original).unwrap();
    source.revalidate_content().unwrap();
    f.assert_document(&source);
}

#[test]
fn runtime_preparation_before_first_capture_preserves_source_without_a_lock() {
    let f = Fixture::new();
    assert!(!f.project.brand_directory().exists());
    f.artifacts()
        .prepare_project_runtime_with_context(&f.original)
        .unwrap();
    assert!(f.project.brand_directory().exists());
    assert!(!f.scope.runtime_root().join("artifact-store.lock").exists());
    assert!(!f.artifacts().manifest_path().exists());
    assert!(!f.scope.runtime_root().join("knowledge").exists());
    let source = f.source();
    let mut writer = f.artifacts().writer_with_context(&f.original).unwrap();
    // Preparation is idempotent and does not contend with the real writer.
    f.artifacts()
        .prepare_project_runtime_with_context(&f.original)
        .unwrap();
    let directory = writer
        .create_record_directory(ProjectRecordDirectory::Knowledge, record_limits())
        .unwrap();
    directory.revalidate(&f.original).unwrap();
    source.revalidate_content().unwrap();
    f.assert_document(&source);
}

#[test]
fn runtime_preparation_reuses_the_physical_scope_with_the_current_context() {
    let f = Fixture::new();
    let current = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ));
    f.artifacts()
        .prepare_project_runtime_with_context(&current)
        .unwrap();
    f.artifacts()
        .prepare_project_runtime_with_context(&f.original)
        .unwrap();
    assert!(f.project.brand_directory().exists());
    assert!(!f.scope.runtime_root().join("artifact-store.lock").exists());
    assert!(!f.artifacts().manifest_path().exists());
}

#[test]
fn runtime_preparation_rejects_cancelled_or_expired_context_before_creation() {
    let f = Fixture::new();
    f.original.cancellation().cancel();
    let error = f
        .artifacts()
        .prepare_project_runtime_with_context(&f.original)
        .unwrap_err();
    assert_eq!(
        error.stage(),
        graphit_core::artifact_store::ArtifactStage::Prepare
    );
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(!f.project.brand_directory().exists());
    let expired = Arc::new(SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    ));
    assert_eq!(
        f.artifacts()
            .prepare_project_runtime_with_context(&expired)
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(!f.project.brand_directory().exists());
}

#[test]
fn runtime_preparation_rejects_replaced_project_before_creation() {
    let f = Fixture::new();
    let retained = f.project.root().with_file_name("retained-project");
    fs::rename(f.project.root(), retained).unwrap();
    fs::create_dir(f.project.root()).unwrap();
    assert_eq!(
        f.artifacts()
            .prepare_project_runtime_with_context(&f.original)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert!(!f.project.brand_directory().exists());
}
