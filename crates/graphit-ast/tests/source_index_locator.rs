use arrow_array::{ArrayRef, RecordBatch, StringArray};
use graphit_ast::{
    indexed_source::{MissingSourceReason, source_schema},
    source_index_locator::{
        LocalSourceIndex, SourceIndexError, SourceIndexPresence, open_local_source_index,
    },
    source_inspection::{IndexedSource, SourceOrigin, inspect_source},
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
    text_slice::Request,
    version_context::VersionContext,
};
use graphit_data::{
    lance_branch_store::{BranchError, LanceBranchStore, cleanup_ast_corpus},
    lance_module_store::LanceModuleStore,
};
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
            .join("source.lance")
    }
    async fn available<'a>(
        &'a self,
        index: usize,
        named: bool,
        request: &Arc<SnapshotContext>,
    ) -> Result<LocalSourceIndex<'a>, Box<dyn Error>> {
        match open_local_source_index(
            self.installation(),
            self.project(index, named),
            request,
            limits(),
        )
        .await?
        {
            SourceIndexPresence::Available(index) => Ok(*index),
            SourceIndexPresence::Absent => Err("expected actual corpus".into()),
        }
    }
}
fn batch(rows: &[(&str, Option<&str>)]) -> RecordBatch {
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(
            rows.iter().map(|r| r.0).collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter().map(|r| r.1).collect::<Vec<_>>(),
        )),
    ];
    RecordBatch::try_new(source_schema(), columns).unwrap()
}
async fn publish(
    fixture: &Fixture,
    index: usize,
    rows: &[(&str, Option<&str>)],
) -> Result<LanceModuleStore, Box<dyn Error>> {
    let path = fixture.corpus(index);
    fs::create_dir_all(path.parent().unwrap())?;
    Ok(
        LanceModuleStore::create(path.to_str().unwrap(), source_schema(), vec![batch(rows)])
            .await?,
    )
}

#[tokio::test]
async fn service_original_retains_source_scope_and_cancellation() -> TestResult {
    let request = Arc::new(SnapshotContext::service(Cancellation::default()));
    let fixture = Fixture::new(&request)?;
    publish(&fixture, 0, &[("src/service.rs", Some("fn service() {}"))]).await?;
    let index = fixture.available(0, false, &request).await?;
    assert!(request.deadline().is_none());
    let raw = index.read("src/service.rs", &request).await?;
    assert!(matches!(
        raw.source(&request)?,
        IndexedSource::Available("fn service() {}")
    ));
    index.revalidate(&request)?;
    request.cancellation().cancel();
    assert!(matches!(
        index.revalidate(&request),
        Err(SourceIndexError::Operation(ref error)) if error.kind() == io::ErrorKind::Interrupted
    ));
    Ok(())
}

#[tokio::test]
async fn absent_components_never_create_storage() -> TestResult {
    let request = original();
    let fixture = Fixture::new(&request)?;
    let path = fixture.corpus(0);
    for parent in [
        path.parent().unwrap().parent().unwrap(),
        path.parent().unwrap(),
    ] {
        assert!(matches!(
            open_local_source_index(
                fixture.installation(),
                fixture.project(0, false),
                &request,
                limits()
            )
            .await?,
            SourceIndexPresence::Absent
        ));
        fs::create_dir_all(parent)?;
        assert!(!path.exists());
    }
    Ok(())
}

#[tokio::test]
async fn raw_empty_null_and_project_ids_remain_distinct() -> TestResult {
    let request = original();
    let fixture = Fixture::new(&request)?;
    publish(
        &fixture,
        0,
        &[
            ("empty", Some("")),
            ("off", None),
            ("same", Some("\u{feff}A\r\n\0🦀")),
        ],
    )
    .await?;
    publish(&fixture, 1, &[("same", Some("other project"))]).await?;
    let first = fixture.available(0, false, &request).await?;
    let second = fixture.available(1, false, &request).await?;
    for (path, text, missing) in [
        ("empty", IndexedSource::Available(""), None),
        (
            "off",
            IndexedSource::Missing,
            Some(MissingSourceReason::TextNotStored),
        ),
        (
            "absent",
            IndexedSource::Missing,
            Some(MissingSourceReason::FileAbsent),
        ),
        ("same", IndexedSource::Available("\u{feff}A\r\n\0🦀"), None),
    ] {
        let result = first.read(path, &request).await?;
        assert_eq!(result.source(&request)?, text);
        assert_eq!(result.missing_reason(), missing);
    }
    let empty = first.read("empty", &request).await?;
    assert_eq!(
        inspect_source(empty.source(&request)?, None, &Request::default(), &request)?.origin,
        SourceOrigin::Indexed
    );
    assert_eq!(
        second.read("same", &request).await?.source(&request)?,
        IndexedSource::Available("other project")
    );
    first.revalidate(&request)?;
    Ok(())
}

#[tokio::test]
async fn selected_default_inherited_named_and_old_generations_are_read_only() -> TestResult {
    let request = original();
    let fixture = Fixture::new(&request)?;
    let base = publish(&fixture, 0, &[("same", Some("A"))]).await?;
    let default = fixture.available(0, false, &request).await?;
    let inherited = fixture.available(0, true, &request).await?;
    let encoded = fixture.projects[0].named.lance_branch();
    assert!(!encoded.is_empty());
    assert_eq!(default.branch_observation().physical_branch, None);
    assert_eq!(default.branch_observation().inherited_target, None);
    assert_eq!(
        inherited.branch_observation().inherited_target,
        Some(encoded.as_str())
    );
    let corpus = fixture.corpus(0);
    let native = lance::Dataset::open(corpus.to_str().unwrap()).await?;
    assert!(matches!(
        native.branches().get(&encoded).await,
        Err(lance::Error::RefNotFound { .. })
    ));
    assert_eq!(
        inherited.read("same", &request).await?.source(&request)?,
        IndexedSource::Available("A")
    );
    assert!(matches!(
        native.branches().get(&encoded).await,
        Err(lance::Error::RefNotFound { .. })
    ));
    // Explicit SDK writer setup, never part of the locator.
    let store = LanceBranchStore::open(corpus.to_str().unwrap(), source_schema(), None).await?;
    let named = store
        .capture_target(Some(&encoded))
        .await?
        .materialize()
        .await?;
    named
        .prepare_merge(vec![batch(&[("same", Some("B"))])], vec!["path".into()])
        .await?
        .commit()
        .await?;
    base.snapshot()
        .prepare_replace(vec![batch(&[("same", Some("C"))])])
        .await?
        .commit()
        .await?;
    let current_named = fixture.available(0, true, &request).await?;
    let current_default = fixture.available(0, false, &request).await?;
    for (index, text) in [
        (&default, "A"),
        (&inherited, "A"),
        (&current_named, "B"),
        (&current_default, "C"),
    ] {
        let result = index.read("same", &request).await?;
        assert_eq!(result.source(&request)?, IndexedSource::Available(text));
        assert_eq!(result.version(), index.version());
    }
    assert_eq!(
        current_named.branch_observation().physical_branch,
        Some(encoded.as_str())
    );
    Ok(())
}

#[tokio::test]
async fn source_snapshots_lease_history_until_the_last_reader_drops() -> TestResult {
    let request = original();
    let fixture = Fixture::new(&request)?;
    let base = publish(&fixture, 0, &[("same", Some("A"))]).await?;
    let default = fixture.available(0, false, &request).await?;
    let retained = default.read("same", &request).await?;
    let encoded = fixture.projects[0].named.lance_branch();
    let path = fixture.corpus(0);
    let store = LanceBranchStore::open(path.to_str().unwrap(), source_schema(), None).await?;
    let named = store
        .capture_target(Some(&encoded))
        .await?
        .materialize()
        .await?;
    named
        .prepare_merge(vec![batch(&[("same", Some("named"))])], vec!["path".into()])
        .await?
        .commit()
        .await?;
    base.snapshot()
        .prepare_replace(vec![batch(&[("same", Some("B"))])])
        .await?
        .commit()
        .await?;
    drop((base, named, store));
    assert!(cleanup_ast_corpus(&path, &request).await?.is_none());
    drop(default);
    assert!(cleanup_ast_corpus(&path, &request).await?.is_none());
    assert_eq!(retained.source(&request)?, IndexedSource::Available("A"));
    drop(retained);
    let cleaned = cleanup_ast_corpus(&path, &request)
        .await?
        .expect("all readers dropped");
    assert!(
        cleaned
            .iter()
            .any(|(before, after)| before.manifests > after.manifests)
    );
    assert!(
        cleaned
            .iter()
            .all(|(before, after)| before.head == after.head)
    );
    let current = fixture.available(0, false, &request).await?;
    let current_named = fixture.available(0, true, &request).await?;
    assert_eq!(
        current.read("same", &request).await?.source(&request)?,
        IndexedSource::Available("B")
    );
    assert_eq!(
        current_named
            .read("same", &request)
            .await?
            .source(&request)?,
        IndexedSource::Available("named")
    );
    Ok(())
}

#[tokio::test]
async fn wrong_kind_and_corrupt_observed_corpus_are_errors_with_causes() -> TestResult {
    let request = original();
    let fixture = Fixture::new(&request)?;
    let ast = fixture.corpus(0).parent().unwrap().to_path_buf();
    fs::create_dir_all(ast.parent().unwrap())?;
    fs::write(&ast, "wrong kind")?;
    assert!(matches!(
        open_local_source_index(
            fixture.installation(),
            fixture.project(0, false),
            &request,
            limits()
        )
        .await,
        Err(SourceIndexError::UnexpectedEntry)
    ));
    fs::remove_file(&ast)?;
    fs::create_dir_all(fixture.corpus(0))?;
    let error = open_local_source_index(
        fixture.installation(),
        fixture.project(0, false),
        &request,
        limits(),
    )
    .await
    .err()
    .unwrap();
    let SourceIndexError::Operation(cause) = &error else {
        panic!("expected retained SDK acquisition failure: {error}");
    };
    let retained = cause
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<BranchError>());
    assert!(matches!(retained, Some(BranchError::Sdk(_))));
    Ok(())
}

#[tokio::test]
async fn scopes_original_checks_and_disappearance_never_become_absence() -> TestResult {
    let request = original();
    let fixture = Fixture::new(&request)?;
    publish(&fixture, 0, &[("file", Some("text"))]).await?;
    let index = fixture.available(0, false, &request).await?;
    assert!(matches!(
        index.revalidate(&original()),
        Err(SourceIndexError::RequestMismatch)
    ));
    let foreign = ProjectCapture {
        version: &fixture.projects[1].default,
        ..fixture.project(0, false)
    };
    assert!(matches!(
        open_local_source_index(fixture.installation(), foreign, &request, limits()).await,
        Err(SourceIndexError::InvalidScope)
    ));
    let mismatched = ProjectCapture {
        scope: ScopeCapture {
            live: &fixture.projects[0].scope,
            record: &fixture.projects[1].record,
        },
        version: &fixture.projects[0].default,
    };
    assert!(matches!(
        open_local_source_index(fixture.installation(), mismatched, &request, limits()).await,
        Err(SourceIndexError::InvalidScope)
    ));
    let cancelled = original();
    cancelled.cancellation().cancel();
    let error = open_local_source_index(
        fixture.installation(),
        fixture.project(0, false),
        &cancelled,
        limits(),
    )
    .await
    .err()
    .unwrap();
    assert!(
        matches!(&error, SourceIndexError::Operation(e) if e.kind() == io::ErrorKind::Interrupted)
    );
    assert!(error.source().is_some());
    let expired = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    assert!(
        matches!(open_local_source_index(fixture.installation(), fixture.project(0, false), &expired, limits()).await, Err(SourceIndexError::Operation(e)) if e.kind() == io::ErrorKind::TimedOut)
    );
    let corpus = fixture.corpus(0);
    let moved = corpus.with_extension("moved");
    #[cfg(not(windows))]
    {
        fs::rename(&corpus, &moved)?;
        fs::create_dir(&corpus)?;
        let error = index.read("file", &request).await.err().unwrap();
        assert!(matches!(error, SourceIndexError::Operation(_)));
        assert!(error.source().is_some());
    }
    #[cfg(windows)]
    {
        let error = fs::rename(&corpus, &moved).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5)); // ERROR_ACCESS_DENIED
        assert!(fs::metadata(&corpus)?.is_dir());
        assert!(!moved.try_exists()?);
        index.revalidate(&request)?;
        assert_eq!(
            index.read("file", &request).await?.source(&request)?,
            IndexedSource::Available("text")
        );
        drop(index);
        fs::rename(&corpus, &moved)?;
        assert!(!corpus.try_exists()?);
        assert!(fs::metadata(&moved)?.is_dir());
    }
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_component_is_refused_without_following_it() -> TestResult {
    let request = original();
    let fixture = Fixture::new(&request)?;
    let target = fixture._temp.path().join("foreign");
    fs::create_dir(&target)?;
    let ast = fixture.corpus(0).parent().unwrap().to_path_buf();
    fs::create_dir_all(ast.parent().unwrap())?;
    std::os::unix::fs::symlink(&target, ast)?;
    assert!(matches!(
        open_local_source_index(
            fixture.installation(),
            fixture.project(0, false),
            &request,
            limits()
        )
        .await,
        Err(SourceIndexError::UnexpectedEntry)
    ));
    assert!(fs::read_dir(target)?.next().is_none());
    Ok(())
}

#[tokio::test]
async fn inventory_retains_scope_original_and_generation_without_bodies() -> TestResult {
    let request = original();
    let fixture = Fixture::new(&request)?;
    let base = publish(&fixture, 0, &[("off", None), ("empty", Some(""))]).await?;
    publish(&fixture, 1, &[("foreign", Some("other project"))]).await?;
    let index = fixture.available(0, false, &request).await?;
    let second = fixture.available(1, false, &request).await?;
    let inherited = fixture.available(0, true, &request).await?;
    assert_eq!(index.paths(&request).await?, vec!["empty", "off"]);
    assert_eq!(second.paths(&request).await?, vec!["foreign"]);
    assert_eq!(inherited.paths(&request).await?, vec!["empty", "off"]);
    assert!(inherited.branch_observation().inherited_target.is_some());
    assert_eq!(
        index.read("off", &request).await?.missing_reason(),
        Some(MissingSourceReason::TextNotStored)
    );
    assert!(matches!(
        index.paths(&original()).await,
        Err(SourceIndexError::RequestMismatch)
    ));
    base.snapshot()
        .prepare_replace(vec![batch(&[("new", None)])])
        .await?
        .commit()
        .await?;
    assert_eq!(index.paths(&request).await?, vec!["empty", "off"]);
    let fresh = fixture.available(0, false, &request).await?;
    assert_eq!(fresh.paths(&request).await?, vec!["new"]);
    let corpus = fixture.corpus(0);
    let moved = corpus.with_extension("moved");
    #[cfg(not(windows))]
    {
        fs::rename(&corpus, &moved)?;
        fs::create_dir(&corpus)?;
        let error = index.paths(&request).await.unwrap_err();
        assert!(matches!(error, SourceIndexError::Operation(_)));
        assert!(error.source().is_some());
    }
    #[cfg(windows)]
    {
        let error = fs::rename(&corpus, &moved).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5)); // ERROR_ACCESS_DENIED
        assert!(fs::metadata(&corpus)?.is_dir());
        assert!(!moved.try_exists()?);
        index.revalidate(&request)?;
        assert_eq!(index.paths(&request).await?, vec!["empty", "off"]);
        assert_eq!(fresh.paths(&request).await?, vec!["new"]);
        assert_eq!(second.paths(&request).await?, vec!["foreign"]);
        inherited.revalidate(&request)?;
        drop((index, second, inherited, fresh, base));
        fs::rename(&corpus, &moved)?;
        assert!(!corpus.try_exists()?);
        assert!(fs::metadata(&moved)?.is_dir());
    }
    Ok(())
}

#[tokio::test]
async fn inventory_control_and_available_empty_are_not_absence() -> TestResult {
    let request = original();
    let fixture = Fixture::new(&request)?;
    publish(&fixture, 0, &[]).await?;
    let index = fixture.available(0, false, &request).await?;
    assert!(index.paths(&request).await?.is_empty());
    assert!(matches!(
        open_local_source_index(
            fixture.installation(),
            fixture.project(1, false),
            &request,
            limits()
        )
        .await?,
        SourceIndexPresence::Absent
    ));
    request.cancellation().cancel();
    let error = index.paths(&request).await.unwrap_err();
    assert!(
        matches!(&error, SourceIndexError::Operation(e) if e.kind() == io::ErrorKind::Interrupted)
    );
    assert!(error.source().is_some());
    Ok(())
}

#[tokio::test]
async fn same_version_raw_replacement_changes_captured_directory_identity() -> TestResult {
    let request = original();
    let fixture = Fixture::new(&request)?;
    publish(&fixture, 0, &[("file", Some("before"))]).await?;
    let before = fixture.available(0, true, &request).await?;
    let identity = before.directory_identity();
    let version = before.version();
    let physical_branch = before
        .branch_observation()
        .physical_branch
        .map(str::to_owned);
    let inherited_target = before
        .branch_observation()
        .inherited_target
        .map(str::to_owned);
    let corpus = fixture.corpus(0);
    let moved = corpus.with_extension("moved");
    #[cfg(windows)]
    {
        let error = fs::rename(&corpus, &moved).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5)); // ERROR_ACCESS_DENIED
        assert!(fs::metadata(&corpus)?.is_dir());
        assert!(!moved.try_exists()?);
        before.revalidate(&request)?;
        assert_eq!(
            before.read("file", &request).await?.source(&request)?,
            IndexedSource::Available("before")
        );
        drop(before);
    }
    fs::rename(&corpus, &moved)?;
    assert!(!corpus.try_exists()?);
    assert!(fs::metadata(&moved)?.is_dir());
    publish(&fixture, 0, &[("file", Some("after"))]).await?;
    let after = fixture.available(0, true, &request).await?;
    assert_eq!(version, after.version());
    assert_eq!(
        physical_branch.as_deref(),
        after.branch_observation().physical_branch
    );
    assert_eq!(
        inherited_target.as_deref(),
        after.branch_observation().inherited_target
    );
    assert!(identity != after.directory_identity());
    #[cfg(not(windows))]
    assert!(before.revalidate(&request).is_err());
    assert_eq!(
        after.read("file", &request).await?.source(&request)?,
        IndexedSource::Available("after")
    );
    after.revalidate(&request)?;
    Ok(())
}
