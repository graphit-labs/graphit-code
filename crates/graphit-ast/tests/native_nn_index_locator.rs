#[path = "support/corpus_fixture.rs"]
mod corpus_fixture;
#[path = "support/native_nn_stream.rs"]
mod native_nn_stream;
#[path = "support/rust_semantic.rs"]
mod rust_semantic;

use arrow_array::{RecordBatch, StringArray};
use graphit_ast::{
    indexed_source::{read_indexed_source, source_schema},
    native_nn::NativeNnModel,
    native_nn_index_locator::{
        LocalNativeNnIndex, NativeNnIndexError, NativeNnIndexPresence, open_local_native_nn_index,
    },
    native_nn_query::{NativeNnMetric, query_corpus_nn},
};
use graphit_auth::{ProjectCapture, ScopeCapture};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeBindingRecord, ScopeLimits, StoreOwner},
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    git_checkout::GitCheckout,
    installation_identity::InstallationIdentityStore,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    version_context::VersionContext,
};
use graphit_data::{lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore};
use std::{
    error::Error,
    fs, io,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;
type TestResult = Result<(), Box<dyn Error>>;
fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ))
}
fn limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
struct ProjectFixture {
    scope: CapturedPublicationScope,
    record: ScopeBindingRecord,
    default: VersionContext,
    named: VersionContext,
}
struct Fixture {
    _temp: TempDir,
    installation: CapturedPublicationScope,
    record: ScopeBindingRecord,
    projects: Vec<ProjectFixture>,
}
impl Fixture {
    fn new(request: &Arc<SnapshotContext>) -> Result<Self, Box<dyn Error>> {
        let temp = TempDir::new()?;
        let paths = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )?;
        let timeout = Duration::from_secs(5);
        let identity =
            InstallationIdentityStore::new(&paths)?.ensure(timeout, request.cancellation())?;
        fs::create_dir_all(paths.global_path(&["runtime"])?)?;
        let scope_limits = ScopeLimits::new(16, 4096, 256)?;
        let mut projects = Vec::new();
        let mut roots = Vec::new();
        for name in ["first", "second"] {
            let root = temp.path().join(name);
            fs::create_dir(&root)?;
            let project = ProjectPaths::at(&root, &paths)?;
            let identity_store = ProjectIdentityStore::at(&project);
            let persisted = identity_store
                .writer(timeout, request.cancellation())?
                .ensure(&ProjectMetadata::new(name)?)?;
            assert_eq!(identity_store.load()?.unwrap().id(), persisted.id());
            fs::create_dir_all(project.project_runtime(persisted.id(), &[])?)?;
            let scope =
                CapturedPublicationScope::project(&project, persisted.id(), scope_limits, request)?;
            let record = scope.binding()?;
            // Real Core version producer observed once in fixture setup. The
            // Source locator receives these contexts and never inspects Git.
            let checkout = GitCheckout::inspect(project.root(), timeout, request.cancellation())?;
            let default = VersionContext::select(&project, checkout.as_checkout(), "", None, "")?;
            let named =
                VersionContext::select(&project, checkout.as_checkout(), "feature", None, "")?;
            roots.push(project.root().to_path_buf());
            projects.push(ProjectFixture {
                scope,
                record,
                default,
                named,
            });
        }
        let installation = CapturedPublicationScope::installation(
            &paths,
            identity.unit_id().clone(),
            roots,
            scope_limits,
            request,
        )?;
        let record = installation.binding()?;
        Ok(Self {
            _temp: temp,
            installation,
            record,
            projects,
        })
    }
    fn installation(&self) -> ScopeCapture<'_> {
        ScopeCapture {
            live: &self.installation,
            record: &self.record,
        }
    }
    fn project(&self, index: usize, named: bool) -> ProjectCapture<'_> {
        let project = &self.projects[index];
        ProjectCapture {
            scope: ScopeCapture {
                live: &project.scope,
                record: &project.record,
            },
            version: if named {
                &project.named
            } else {
                &project.default
            },
        }
    }
    fn corpus(&self, index: usize) -> PathBuf {
        let StoreOwner::Project { project_id } = self.projects[index].scope.owner() else {
            panic!("actual project owner")
        };
        self.installation
            .owner_root()
            .join("projects")
            .join(project_id.to_string())
            .join("ast")
            .join("corpus.lance")
    }
    async fn available<'a>(
        &'a self,
        index: usize,
        named: bool,
        context: &Arc<SnapshotContext>,
    ) -> Result<LocalNativeNnIndex<'a>, Box<dyn Error>> {
        match open_local_native_nn_index(
            self.installation(),
            self.project(index, named),
            context,
            limits(),
        )
        .await?
        {
            NativeNnIndexPresence::Available(owner) => Ok(*owner),
            NativeNnIndexPresence::Absent => Err("expected real NN corpus".into()),
        }
    }
}
fn model() -> NativeNnModel<'static> {
    NativeNnModel {
        vector_id: [123; 32],
        dimensions: 3,
        model_id: "locator/fixture",
        revision: "r1",
    }
}
async fn encoded(
    fixture: &Fixture,
    name: &str,
    expected: &NativeNnModel<'_>,
    context: &Arc<SnapshotContext>,
) -> Result<RecordBatch, Box<dyn Error>> {
    let text = format!("mod outer {{ pub fn {name}() {{}} }}");
    let raw = LanceModuleStore::create(
        fixture
            ._temp
            .path()
            .join(format!("raw-{name}.lance"))
            .to_str()
            .unwrap(),
        source_schema(),
        vec![RecordBatch::try_new(
            source_schema(),
            vec![
                Arc::new(StringArray::from(vec!["src/fixture.rs"])),
                Arc::new(StringArray::from(vec![text.as_str()])),
            ],
        )?],
    )
    .await?;
    let raw = read_indexed_source(&raw.snapshot(), "src/fixture.rs", context).await?;
    corpus_fixture::encode(&raw, &["src", "fixture.rs"], expected, 11, context)
}
async fn create(
    fixture: &Fixture,
    index: usize,
    batch: &RecordBatch,
) -> Result<LanceModuleStore, Box<dyn Error>> {
    let path = fixture.corpus(index);
    fs::create_dir_all(path.parent().unwrap())?;
    Ok(
        LanceModuleStore::create(path.to_str().unwrap(), batch.schema(), vec![batch.clone()])
            .await?,
    )
}
async fn assert_query(
    index: &LocalNativeNnIndex<'_>,
    expected: &NativeNnModel<'_>,
    batch: &RecordBatch,
    context: &Arc<SnapshotContext>,
) -> TestResult {
    let snapshot = index.snapshot(context)?;
    assert_eq!(index.version(), snapshot.version());
    assert_eq!(index.schema(), batch.schema().as_ref());
    graphit_ast::corpus::validate_schema(index.schema())?;
    assert!(index.version() > 0);
    let query = vec![1.; usize::try_from(expected.dimensions)?];
    let result =
        query_corpus_nn(snapshot, expected, &query, 1, NativeNnMetric::L2, context).await?;
    if batch.num_rows() == 0 {
        assert!(
            result
                .rows(context)?
                .iter()
                .all(|batch| batch.num_rows() == 0)
        );
    } else {
        assert_eq!(
            result.rows(context)?,
            &[graphit_ast::corpus::vectors(batch, expected, context)?]
        );
        assert_eq!(result.distances(context)?, &[vec![0.]]);
    }
    let mismatched = NativeNnModel {
        vector_id: expected.vector_id,
        dimensions: expected.dimensions,
        model_id: expected.model_id,
        revision: "unrelated-revision",
    };
    assert!(
        matches!(
            query_corpus_nn(
                snapshot,
                &mismatched,
                &query,
                1,
                NativeNnMetric::L2,
                context
            )
            .await,
            Err(graphit_ast::native_nn_query::NativeNnQueryError::Payload(
                graphit_ast::native_nn::NativeNnError::SchemaMismatch
            ))
        ),
        "available candidates must not hide an incompatible model descriptor"
    );
    index.revalidate(context)?;
    Ok(())
}
fn assert_operation<T>(result: Result<T, NativeNnIndexError>, kind: io::ErrorKind) {
    let error = result.err().expect("real operation error required");
    let NativeNnIndexError::Operation(cause) = &error else {
        panic!("real IO cause required: {error}")
    };
    assert_eq!(cause.kind(), kind);
    assert!(error.source().unwrap().is::<io::Error>());
}

#[tokio::test]
async fn missing_segments_never_create_and_available_empty_keeps_complete_identity() -> TestResult {
    let context = original();
    let fixture = Fixture::new(&context)?;
    let path = fixture.corpus(0);
    for parent in [
        None,
        Some(path.parent().unwrap().parent().unwrap()),
        Some(path.parent().unwrap()),
    ] {
        if let Some(parent) = parent {
            fs::create_dir_all(parent)?;
        }
        assert!(matches!(
            open_local_native_nn_index(
                fixture.installation(),
                fixture.project(0, false),
                &context,
                limits()
            )
            .await?,
            NativeNnIndexPresence::Absent
        ));
        assert!(!path.exists());
    }
    fs::create_dir(&path)?;
    assert!(matches!(
        open_local_native_nn_index(
            fixture.installation(),
            fixture.project(0, false),
            &context,
            limits(),
        )
        .await?,
        NativeNnIndexPresence::Absent
    ));
    assert!(fs::read_dir(&path)?.next().is_none());
    let empty = RecordBatch::new_empty(corpus_fixture::schema(&model())?);
    create(&fixture, 0, &empty).await?;
    let index = fixture.available(0, false, &context).await?;
    assert_query(&index, &model(), &empty, &context).await?;
    assert_eq!(index.physical_branch(), None);
    assert_eq!(index.inherited_target(), None);
    Ok(())
}

#[tokio::test]
async fn selected_two_project_default_named_and_inherited_schemas_are_actual() -> TestResult {
    let context = original();
    let fixture = Fixture::new(&context)?;
    let expected = model();
    let first = encoded(&fixture, "first", &expected, &context).await?;
    let second = encoded(&fixture, "second", &expected, &context).await?;
    create(&fixture, 0, &first).await?;
    create(&fixture, 1, &second).await?;
    let first_index = fixture.available(0, false, &context).await?;
    let second_index = fixture.available(1, false, &context).await?;
    assert_query(&first_index, &expected, &first, &context).await?;
    assert_query(&second_index, &expected, &second, &context).await?;
    let inherited = fixture.available(0, true, &context).await?;
    assert_eq!(inherited.physical_branch(), None);
    assert_eq!(inherited.inherited_target(), Some("feature"));
    assert_query(&inherited, &expected, &first, &context).await?;
    let path = fixture.corpus(0);
    let branches = LanceBranchStore::open(path.to_str().unwrap(), first.schema(), None).await?;
    assert!(
        branches
            .capture_target(Some("feature"))
            .await?
            .inherited_target()
            .is_some()
    );
    let named = branches
        .capture_target(Some("feature"))
        .await?
        .materialize()
        .await?;
    let changed = NativeNnModel {
        vector_id: [124; 32],
        dimensions: 4,
        revision: "r2",
        ..model()
    };
    let new = encoded(&fixture, "named", &changed, &context).await?;
    corpus_fixture::publish(
        &named,
        native_nn_stream::batches(vec![new.clone()], corpus_fixture::schema(&changed)?),
        &changed,
        &context,
    )
    .await?;
    let named_index = fixture.available(0, true, &context).await?;
    assert_eq!(named_index.physical_branch(), Some("feature"));
    assert_eq!(named_index.inherited_target(), None);
    assert_query(&named_index, &changed, &new, &context).await?;
    assert_query(
        &fixture.available(0, false, &context).await?,
        &expected,
        &first,
        &context,
    )
    .await?;
    assert_query(&inherited, &expected, &first, &context).await?;
    assert!(
        query_corpus_nn(
            named_index.snapshot(&context)?,
            &expected,
            &[1.; 3],
            1,
            NativeNnMetric::L2,
            &context
        )
        .await
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn overwrite_new_width_reopen_does_not_refresh_the_retained_owner() -> TestResult {
    let context = original();
    let fixture = Fixture::new(&context)?;
    let expected = model();
    let before = encoded(&fixture, "before", &expected, &context).await?;
    create(&fixture, 0, &before).await?;
    let old = fixture.available(0, false, &context).await?;
    let old_snapshot = old.snapshot(&context)?;
    assert!(std::ptr::eq(old_snapshot, old.snapshot(&context)?));
    let version = old.version();
    let changed = NativeNnModel {
        vector_id: [125; 32],
        dimensions: 5,
        revision: "r3",
        ..model()
    };
    let after = encoded(&fixture, "after", &changed, &context).await?;
    let published = corpus_fixture::publish(
        old_snapshot,
        native_nn_stream::batches(vec![after.clone()], corpus_fixture::schema(&changed)?),
        &changed,
        &context,
    )
    .await?;
    assert!(published.version() > version);
    old.revalidate(&context)?;
    assert_eq!(old.version(), version);
    assert_query(&old, &expected, &before, &context).await?;
    let new = fixture.available(0, false, &context).await?;
    assert_eq!(new.version(), published.version());
    assert_query(&new, &changed, &after, &context).await?;
    Ok(())
}

#[tokio::test]
async fn present_corruption_wrong_entry_and_real_scope_mismatches_remain_errors() -> TestResult {
    let context = original();
    let fixture = Fixture::new(&context)?;
    let path = fixture.corpus(0);
    fs::create_dir_all(&path)?;
    create(
        &fixture,
        0,
        &RecordBatch::new_empty(corpus_fixture::schema(&model())?),
    )
    .await?;
    let dataset = lance::Dataset::open(path.to_str().ok_or("fixture URI")?).await?;
    dataset
        .object_store(None)
        .await?
        .put(&dataset.manifest_location().path, b"corrupt")
        .await?;
    let error = open_local_native_nn_index(
        fixture.installation(),
        fixture.project(0, false),
        &context,
        limits(),
    )
    .await
    .err()
    .expect("unreadable published manifest cannot become absent");
    assert!(matches!(&error, NativeNnIndexError::Storage(cause)
        if matches!(cause.as_ref(), graphit_data::lance_branch_store::BranchError::Sdk(sdk)
            if !matches!(sdk.as_ref(), lance::Error::DatasetNotFound { .. }))));
    assert!(error.source().is_some());
    let fixture = Fixture::new(&context)?;
    let path = fixture.corpus(0);
    fs::create_dir_all(path.parent().ok_or("project directory")?)?;
    fs::write(&path, b"not a directory")?;
    assert!(matches!(
        open_local_native_nn_index(
            fixture.installation(),
            fixture.project(0, false),
            &context,
            limits()
        )
        .await,
        Err(NativeNnIndexError::UnexpectedEntry)
    ));
    let first = fixture.project(0, false);
    let second = fixture.project(1, false);
    let foreign_record = ProjectCapture {
        scope: ScopeCapture {
            live: first.scope.live,
            record: second.scope.record,
        },
        version: first.version,
    };
    let foreign_version = ProjectCapture {
        version: second.version,
        ..first
    };
    for project in [foreign_record, foreign_version] {
        assert!(matches!(
            open_local_native_nn_index(fixture.installation(), project, &context, limits()).await,
            Err(NativeNnIndexError::InvalidScope)
        ));
    }
    assert!(matches!(
        open_local_native_nn_index(first.scope, first, &context, limits()).await,
        Err(NativeNnIndexError::InvalidScope)
    ));
    Ok(())
}

#[tokio::test]
async fn original_controls_and_real_directory_replacement_reject_stale_owner() -> TestResult {
    let context = Arc::new(SnapshotContext::service(Cancellation::default()));
    let fixture = Fixture::new(&context)?;
    let batch = encoded(&fixture, "service", &model(), &context).await?;
    create(&fixture, 0, &batch).await?;
    let index = fixture.available(0, false, &context).await?;
    assert!(context.deadline().is_none());
    assert_query(&index, &model(), &batch, &context).await?;
    let foreign = original();
    assert!(matches!(
        index.snapshot(&foreign),
        Err(NativeNnIndexError::RequestMismatch)
    ));
    let path = fixture.corpus(0);
    let moved_corpus = path.with_file_name("moved.lance");
    #[cfg(not(windows))]
    {
        fs::rename(&path, &moved_corpus)?;
        fs::create_dir(&path)?;
        let error = index
            .snapshot(&context)
            .err()
            .expect("actual directory replacement must reject");
        assert!(matches!(&error, NativeNnIndexError::Operation(_)));
        assert!(error.source().unwrap().is::<io::Error>());
    }
    #[cfg(windows)]
    {
        let error = fs::rename(&path, &moved_corpus).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5)); // ERROR_ACCESS_DENIED
        assert!(fs::metadata(&path)?.is_dir());
        assert!(!moved_corpus.try_exists()?);
        index.revalidate(&context)?;
        assert_query(&index, &model(), &batch, &context).await?;
    }
    // Keep the selected NN directory inode, but replace its real parent.
    // The Core capture retains ancestor identities as well as the selected root.
    let ancestor_batch = encoded(&fixture, "ancestor", &model(), &context).await?;
    create(&fixture, 1, &ancestor_batch).await?;
    let ancestor = fixture.available(1, false, &context).await?;
    let selected = fixture.corpus(1);
    let parent = selected.parent().unwrap();
    let moved = parent.with_file_name("moved-project");
    #[cfg(not(windows))]
    {
        fs::rename(parent, &moved)?;
        fs::create_dir(parent)?;
        fs::rename(moved.join("corpus.lance"), &selected)?;
        assert_operation(ancestor.revalidate(&context), io::ErrorKind::InvalidData);
    }
    #[cfg(windows)]
    {
        let error = fs::rename(parent, &moved).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5)); // ERROR_ACCESS_DENIED
        assert!(fs::metadata(parent)?.is_dir());
        assert!(!moved.try_exists()?);
        ancestor.revalidate(&context)?;
        assert_query(&ancestor, &model(), &ancestor_batch, &context).await?;
    }

    context.cancellation().cancel();
    assert_operation(index.revalidate(&context), io::ErrorKind::Interrupted);
    assert_operation(
        open_local_native_nn_index(
            fixture.installation(),
            fixture.project(0, false),
            &context,
            limits(),
        )
        .await,
        io::ErrorKind::Interrupted,
    );
    let expired = Arc::new(SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    ));
    assert_operation(
        open_local_native_nn_index(
            fixture.installation(),
            fixture.project(0, false),
            &expired,
            limits(),
        )
        .await,
        io::ErrorKind::TimedOut,
    );
    #[cfg(windows)]
    {
        // The same namespace moves succeed once the final native owners leave.
        drop((index, ancestor));
        fs::rename(&path, &moved_corpus)?;
        assert!(!path.try_exists()?);
        assert!(fs::metadata(&moved_corpus)?.is_dir());
        fs::rename(parent, &moved)?;
        assert!(!parent.try_exists()?);
        assert!(fs::metadata(moved.join("corpus.lance"))?.is_dir());
    }
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn literal_symlink_segments_are_never_absent_or_followed() -> TestResult {
    use std::os::unix::fs::symlink;
    let context = original();
    let fixture = Fixture::new(&context)?;
    let path = fixture.corpus(0);
    let ast = path.parent().unwrap();
    fs::create_dir_all(ast.parent().unwrap())?;
    let outside = fixture._temp.path().join("outside");
    fs::create_dir(&outside)?;
    symlink(&outside, ast)?;
    assert!(matches!(
        open_local_native_nn_index(
            fixture.installation(),
            fixture.project(0, false),
            &context,
            limits()
        )
        .await,
        Err(NativeNnIndexError::UnexpectedEntry)
    ));
    fs::remove_file(ast)?;
    fs::create_dir_all(path.parent().unwrap())?;
    symlink(&outside, &path)?;
    assert!(matches!(
        open_local_native_nn_index(
            fixture.installation(),
            fixture.project(0, false),
            &context,
            limits()
        )
        .await,
        Err(NativeNnIndexError::UnexpectedEntry)
    ));
    Ok(())
}
