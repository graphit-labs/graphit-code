use arrow_array::{ArrayRef, RecordBatch, StringArray};
use graphit_ast::{
    imported_source::{
        ImportedSourceError, ImportedSourceInput, inspect_imported_source_local,
        inspect_imported_source_runtime,
    },
    indexed_source::{IndexedSourceError, source_schema},
    source_inspection::{SourceInspectionError, SourceOrigin},
};
use graphit_auth::{
    AdminSessionPolicy, AuthError, IssuerAuthorityPolicy, IssuerLimits, Permission, ProjectCapture,
    RoleIssuer, ScopeCapture, ServiceAdmissionLimits, ServiceAuthorityInput, SourceAuthorityPolicy,
    TtlPolicy,
    profile::{
        AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
        ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, CopyPolicy, RetainedBytesLimits,
        RetainedFileSet, ScopeBindingRecord, ScopeLimits, SnapshotPolicy, SourceReadLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect},
    brand::Brand,
    file_lock::Cancellation,
    git_checkout::GitCheckout,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationIdentityStore,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    native_path::NativePath,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    text_slice::Request,
    version_context::VersionContext,
};
use graphit_data::lance_module_store::{LanceModuleStore, StoreError};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, ProjectionLimits, SourceRef,
    materialization::{LineageLimits, MaterializationLimits},
    native::ContentLimits,
    policy::RequestContext,
    registry::{
        DocumentLimits, HubLimits, HubRegistry, Installation, InstalledArtifact, InstalledBackend,
        InstalledPayload,
    },
};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;
type TestResult = Result<(), Box<dyn Error>>;
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    request_until(cancel, Instant::now() + Duration::from_secs(120))
}
fn request_until(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
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
fn scope_limits() -> ScopeLimits {
    ScopeLimits::new(16, 4096, 256).unwrap()
}
fn index_limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
fn path_limits() -> RetainedBytesLimits {
    RetainedBytesLimits::new(65536, 32, 4096).unwrap()
}
fn file_policy() -> SnapshotPolicy {
    let retention = path_limits();
    SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(1048576, 128, 65536, 8).unwrap(),
        RetainedFileSet::new([], retention).unwrap(),
        retention,
    )
    .unwrap()
}
fn read_limits() -> SourceReadLimits {
    SourceReadLimits::new(65536, 4096, 1048576).unwrap()
}
fn issuer_limits() -> IssuerLimits {
    IssuerLimits {
        max_permissions: 64,
        max_scope_bytes: 65536,
        max_parent_bearer_bytes: 4096,
    }
}
fn authority(read: bool) -> IssuerAuthorityPolicy {
    let ordinary = if read {
        vec![Permission::Read(Module::Ast)]
    } else {
        vec![]
    };
    let administrative = [
        Permission::Read(Module::Ast),
        Permission::ManageAccessGrants,
    ];
    let source = || SourceAuthorityPolicy {
        ordinary: &ordinary,
        administrative: &administrative,
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    IssuerAuthorityPolicy::new(
        source(),
        source(),
        issuer_limits(),
        ServiceAuthorityInput {
            rules: &[],
            limits: ServiceAdmissionLimits {
                max_rules: 16,
                max_binding_bytes: 8192,
            },
            native_refresh_margin: Duration::ZERO,
        },
    )
    .unwrap()
}
struct ProjectFixture {
    root: PathBuf,
    scope: CapturedPublicationScope,
    record: ScopeBindingRecord,
    named: VersionContext,
}
struct Fixture {
    issuer: RoleIssuer,
    paths: InstallationPaths,
    registry: HubRegistry,
    installation: CapturedPublicationScope,
    record: ScopeBindingRecord,
    projects: Vec<ProjectFixture>,
    raw: String,
    _temp: TempDir,
}
impl Fixture {
    fn new(ctx: &RequestContext<'_>, read: bool, local: bool) -> Result<Self, Box<dyn Error>> {
        let temp = TempDir::new()?;
        let paths = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )?;
        let original = ctx.snapshot();
        let private =
            installation_bootstrap::prepare(&paths, BootstrapLimits::new(65536, 256)?, original)
                .into_installation()?;
        let identity = InstallationIdentityStore::new(&paths)?
            .ensure(Duration::from_secs(5), ctx.cancellation())?;
        let started =
            RuntimeKeyOwner::start(private, RuntimeKeyLimits::new(65536, 256, 65536)?, original);
        assert!(started.failure().is_none());
        assert!(started.cleanup_failure().is_none());
        let runtime = Arc::new(started.into_owner()?);
        let mut projects = Vec::new();
        let mut roots = Vec::new();
        for name in ["first", "second"] {
            let root = temp.path().join(name);
            fs::create_dir(&root)?;
            let project = ProjectPaths::at(&root, &paths)?;
            let persisted = ProjectIdentityStore::at(&project)
                .writer(Duration::from_secs(5), ctx.cancellation())?
                .ensure(&ProjectMetadata::new(name)?)?;
            fs::create_dir_all(project.project_runtime(persisted.id(), &[])?)?;
            let scope = CapturedPublicationScope::project(
                &project,
                persisted.id(),
                scope_limits(),
                original,
            )?;
            let record = scope.binding()?;
            // Setup observes the actual Core producer once. The handler uses
            // supplied resolved roots/versions and performs no Git lookup.
            let checkout = GitCheckout::inspect(&root, Duration::from_secs(5), ctx.cancellation())?;
            let named =
                VersionContext::select(&project, checkout.as_checkout(), "feature", None, "")?;
            roots.push(root.clone());
            projects.push(ProjectFixture {
                root,
                scope,
                record,
                named,
            });
        }
        let installation = CapturedPublicationScope::installation(
            &paths,
            identity.unit_id().clone(),
            roots.clone(),
            scope_limits(),
            original,
        )?;
        let record = installation.binding()?;
        let store_scope = CapturedPublicationScope::installation(
            &paths,
            identity.unit_id().clone(),
            [paths.root().to_path_buf()],
            scope_limits(),
            original,
        )?;
        let store = Arc::new(AuthStore::new(
            AuthDocumentStore::capture(
                store_scope,
                AuthDocumentLimits::new(65536, 65536, 256, 65536)?,
                original,
            )?,
            ProfileLimits {
                bytes: 65536,
                nodes: 4096,
                scalar_bytes: 4096,
                collection_entries: 64,
                providers: 16,
                profiles: 16,
            },
            ctx,
        )?);
        if local {
            publish_local(&store, ctx)?;
        }
        let issuer = RoleIssuer::new(
            ScopeCapture {
                live: &installation,
                record: &record,
            },
            Arc::clone(&runtime),
            store,
            authority(read),
            TtlPolicy {
                ttl: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
                refresh_margin: Duration::from_secs(120),
            },
            ctx,
        )?;
        let checked = runtime.check(original)?;
        let raw = String::from_utf8(checked.secret(original)?.to_vec())?;
        Ok(Self {
            issuer,
            registry: HubRegistry::global(&paths, identity.unit_id(), hub_limits())?,
            paths,
            installation,
            record,
            projects,
            raw,
            _temp: temp,
        })
    }

    fn input<'a, 'r>(
        &'a self,
        ctx: &'a RequestContext<'a>,
        source: &'r SourceRef,
        selection: &'r Request,
        policy: &'r SnapshotPolicy,
        fallback: Option<usize>,
    ) -> ImportedSourceInput<'a, 'r>
    where
        'a: 'r,
    {
        ImportedSourceInput {
            registry: &self.registry,
            id: source.id(),
            version: source.version(),
            installation: ScopeCapture {
                live: &self.installation,
                record: &self.record,
            },
            fallback: fallback.map(|index| {
                let p = &self.projects[index];
                ProjectCapture {
                    scope: ScopeCapture {
                        live: &p.scope,
                        record: &p.record,
                    },
                    version: &p.named,
                }
            }),
            requested_relative: Path::new("file.any"),
            selection,
            file_policy: policy,
            read_limits: read_limits(),
            path_limits: path_limits(),
            index_limits: index_limits(),
            request: ctx,
        }
    }
    fn source(&self, origin: u8, id: &str, version: &str) -> Result<SourceRef, Box<dyn Error>> {
        let limits = ProjectionLimits::new(4096, 32768, 128, 8192, 128)?;
        let id = ArtifactId::new(id, limits)?;
        let version = ConcreteVersion::new(version, limits)?;
        let graphit_core::artifact_store::StoreOwner::Project { project_id } =
            self.projects[0].scope.owner()
        else {
            unreachable!()
        };
        let producer = *project_id;
        Ok(match origin {
            0 => SourceRef::local_link(
                NativePath::new(self._temp.path().join(id.as_str()))?,
                producer,
                ArtifactType::Ast,
                id,
                version,
                limits,
            )?,
            1 => SourceRef::resources(
                &ProjectPaths::at(&self.projects[0].root, &self.paths)?,
                producer,
                ArtifactType::Ast,
                id,
                version,
                limits,
            )?,
            _ => SourceRef::hub_cache(
                &self.paths,
                producer,
                ArtifactType::Ast,
                id,
                None,
                version,
                limits,
            )?,
        })
    }
    fn register(&self, source: &SourceRef, ctx: &RequestContext<'_>) -> TestResult {
        self.registry
            .writer(Duration::from_secs(5), ctx.cancellation())?
            .put_metadata(InstalledArtifact {
                consumer: self.installation.publication_slot(),
                artifact_type: ArtifactType::Ast,
                id: source.id().clone(),
                installations: vec![Installation {
                    target: InstallationTarget::GlobalNative,
                    original_source: source.clone(),
                    alias: None,
                    entry_name: None,
                    display: None,
                    backend: InstalledBackend::Native,
                    payload: InstalledPayload::Original {
                        selection_binding: None,
                    },
                }],
            })?;
        Ok(())
    }
}
fn hub_limits() -> HubLimits {
    let copy = CopyLimits::new(16777216, 512, 32768, 16).unwrap();
    HubLimits::new(
        DocumentLimits {
            bytes: 262144,
            entries: 128,
            key_bytes: 8192,
            json_nodes: 50000,
            json_string_bytes: 32768,
        },
        ProjectionLimits::new(4096, 32768, 128, 8192, 128).unwrap(),
        ArtifactLimits::new(262144, 8192, copy).unwrap(),
        MaterializationLimits {
            output: copy,
            snapshots: copy,
            lineage: LineageLimits::new(4096, 8192, 32768).unwrap(),
        },
    )
    .unwrap()
}
fn publish_local(store: &AuthStore, ctx: &RequestContext<'_>) -> TestResult {
    let before = store.load(ctx)?;
    let result = store.mutate(before, ctx, |doc, ctx| {
        doc.add_provider(
            "local".into(),
            ProviderConfig {
                kind: ProviderKind::Local {
                    allow_aws_credential_chain: false,
                    broker: None,
                },
                embedding: AiService::Disabled,
                rerank: AiService::Disabled,
                s3: S3Location {
                    bucket: None,
                    region: None,
                    endpoint: None,
                    prefix: None,
                    source: S3Source::LocalCredentials,
                },
            },
            doc.revision(),
            1,
            ctx,
        )?;
        let expected = ProfileExpectation {
            document_revision: doc.revision(),
            provider_revision: doc.provider("local").unwrap().revision(),
            profile_revision: None,
        };
        doc.replace_profile_data(
            ProfileData {
                name: "actual-local".into(),
                provider: "local".into(),
                username: "fixture".into(),
                organization: None,
                teams: vec![],
                credentials: ProfileCredentials::Local(LocalCredentials {
                    anonymous: false,
                    mcp_key: Some(Secret::new("test-local-key".into())),
                    broker_key: None,
                    embedding_api_key: None,
                    rerank_api_key: None,
                    s3: None,
                }),
            },
            expected,
            1,
            ctx,
        )?;
        doc.activate("actual-local", doc.revision(), ctx)?;
        Ok(())
    })?;
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    Ok(())
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
async fn corpus(
    source: &SourceRef,
    rows: &[(&str, Option<&str>)],
) -> Result<LanceModuleStore, Box<dyn Error>> {
    fs::create_dir_all(source.directory().as_path().join("graph.icebug"))?;
    let path = source
        .directory()
        .as_path()
        .join("graph.icebug/source.lance");
    Ok(
        LanceModuleStore::create(path.to_str().unwrap(), source_schema(), vec![batch(rows)])
            .await?,
    )
}

#[tokio::test]
async fn service_original_keeps_imported_source_authority_and_cancellation() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = RequestContext::service(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        &cancel,
    )?;
    let fixture = Fixture::new(&ctx, true, false)?;
    let source = fixture.source(0, "service/source", "v1")?;
    corpus(&source, &[("file.any", Some("retained service source"))]).await?;
    fixture.register(&source, &ctx)?;
    let selection = Request::default();
    let policy = file_policy();
    assert!(ctx.snapshot().deadline().is_none());
    let found = inspect_imported_source_runtime(
        &fixture.issuer,
        &fixture.raw,
        fixture.input(&ctx, &source, &selection, &policy, None),
    )
    .await?;
    assert_eq!(found.origin, SourceOrigin::Indexed);
    assert_eq!(found.selection.source, "retained service source");
    cancel.cancel();
    assert!(
        inspect_imported_source_runtime(
            &fixture.issuer,
            &fixture.raw,
            fixture.input(&ctx, &source, &selection, &policy, None),
        )
        .await
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn exact_namespaces_origins_and_global_native_target() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, true)?;
    let selection = Request::default();
    let policy = file_policy();
    for (origin, id, text) in [
        (0, "alpha/shared", "alpha"),
        (1, "beta/shared", "beta"),
        (2, "gamma/shared", "gamma"),
    ] {
        let source = f.source(origin, id, "release-1")?;
        let store = corpus(&source, &[("file.any", Some(text))]).await?;
        f.register(&source, &ctx)?;
        let before = store.snapshot().version();
        let input = f.input(&ctx, &source, &selection, &policy, None);
        let found = inspect_imported_source_runtime(&f.issuer, &f.raw, input).await?;
        assert_eq!(found.origin, SourceOrigin::Indexed);
        assert_eq!(found.selection.source, text);
        let local = inspect_imported_source_local(
            &f.issuer,
            "test-local-key",
            f.input(&ctx, &source, &selection, &policy, None),
        )
        .await?;
        assert_eq!(local.selection.source, text);
        assert_eq!(
            LanceModuleStore::open(
                source
                    .directory()
                    .as_path()
                    .join("graph.icebug/source.lance")
                    .to_str()
                    .unwrap(),
                source_schema()
            )
            .await?
            .snapshot()
            .version(),
            before
        );
    }
    Ok(())
}

#[tokio::test]
async fn empty_raw_wins_and_only_explicit_misses_use_independent_fallback() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    let selection = Request {
        start_line: 2,
        end_line: 2,
        line_numbers: true,
        ..Request::default()
    };
    let policy = file_policy();
    fs::write(f.projects[0].root.join("file.any"), "first\nphysical\nlast")?;
    for (id, rows) in [
        ("empty", vec![("file.any", Some(""))]),
        ("null", vec![("file.any", None)]),
        ("absent-row", vec![("other", Some("ignored"))]),
        ("absent-corpus", vec![]),
    ] {
        let source = f.source(0, id, "artifact-version-unrelated-to-feature")?;
        if rows.is_empty() {
            fs::create_dir_all(source.directory().as_path().join("graph.icebug"))?;
        } else {
            corpus(&source, &rows).await?;
        }
        f.register(&source, &ctx)?;
        let default = Request::default();
        let result = inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &source, &default, &policy, None),
        )
        .await;
        if id == "empty" {
            let result = result?;
            assert_eq!(result.origin, SourceOrigin::Indexed);
            assert_eq!(result.selection.source, "");
        } else {
            assert!(matches!(
                result,
                Err(ImportedSourceError::SourceUnavailable)
            ));
            let result = inspect_imported_source_runtime(
                &f.issuer,
                &f.raw,
                f.input(&ctx, &source, &selection, &policy, Some(0)),
            )
            .await?;
            assert_eq!(result.origin, SourceOrigin::Physical);
            assert_eq!(result.selection.source, "   2: physical");
            assert!(matches!(
                inspect_imported_source_runtime(
                    &f.issuer,
                    &f.raw,
                    f.input(&ctx, &source, &selection, &policy, Some(1))
                )
                .await,
                Err(ImportedSourceError::InvalidFallback)
            ));
        }
    }
    Ok(())
}

#[tokio::test]
async fn wrong_version_and_paths_do_not_select_other_installations() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    let source = f.source(0, "namespace/name", "v1")?;
    f.register(&source, &ctx)?;
    let selection = Request::default();
    let policy = file_policy();
    let limits = ProjectionLimits::new(4096, 32768, 128, 8192, 128)?;
    let version = ConcreteVersion::new("v2", limits)?;
    let mut input = f.input(&ctx, &source, &selection, &policy, None);
    input.version = &version;
    assert!(matches!(
        inspect_imported_source_runtime(&f.issuer, &f.raw, input).await,
        Err(ImportedSourceError::InstallationNotFound)
    ));
    for path in ["", "../file.any", "/file.any"] {
        let mut input = f.input(&ctx, &source, &selection, &policy, None);
        input.requested_relative = Path::new(path);
        assert!(
            inspect_imported_source_runtime(&f.issuer, &f.raw, input)
                .await
                .is_err()
        );
    }
    // Root absence is a broken origin, never permission for a disk fallback.
    assert!(matches!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &source, &selection, &policy, Some(0))
        )
        .await,
        Err(ImportedSourceError::Operation(_))
    ));
    Ok(())
}

#[tokio::test]
async fn schema_corruption_and_duplicate_rows_never_fall_back() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    let selection = Request::default();
    let policy = file_policy();
    fs::write(f.projects[0].root.join("file.any"), "disk")?;
    let source = f.source(0, "duplicate", "v1")?;
    corpus(
        &source,
        &[("file.any", Some("one")), ("file.any", Some("two"))],
    )
    .await?;
    f.register(&source, &ctx)?;
    assert!(matches!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &source, &selection, &policy, Some(0))
        )
        .await,
        Err(ImportedSourceError::Indexed(
            IndexedSourceError::DuplicatePath
        ))
    ));
    let bad = f.source(0, "bad-sdk", "v1")?;
    fs::create_dir_all(bad.directory().as_path().join("graph.icebug/source.lance"))?;
    f.register(&bad, &ctx)?;
    assert!(matches!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &bad, &selection, &policy, Some(0))
        )
        .await,
        Err(ImportedSourceError::Storage(StoreError::Sdk(_)))
    ));
    let wrong = f.source(0, "wrong-schema", "v1")?;
    fs::create_dir_all(wrong.directory().as_path().join("graph.icebug"))?;
    let schema = Arc::new(arrow_schema::Schema::new(vec![arrow_schema::Field::new(
        "other",
        arrow_schema::DataType::Utf8,
        false,
    )]));
    LanceModuleStore::create(
        wrong
            .directory()
            .as_path()
            .join("graph.icebug/source.lance")
            .to_str()
            .unwrap(),
        Arc::clone(&schema),
        vec![RecordBatch::new_empty(schema)],
    )
    .await?;
    f.register(&wrong, &ctx)?;
    assert!(matches!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &wrong, &selection, &policy, Some(0))
        )
        .await,
        Err(ImportedSourceError::Storage(StoreError::SchemaMismatch))
    ));
    Ok(())
}

#[tokio::test]
async fn auth_precedes_malformed_registry_and_origin_access() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, false, true)?;
    let source = f.source(0, "denied", "v1")?;
    f.register(&source, &ctx)?;
    fs::write(f.registry.path(), "malformed registry")?;
    let selection = Request::default();
    let policy = file_policy();
    assert!(matches!(
        inspect_imported_source_runtime(
            &f.issuer,
            "invalid",
            f.input(&ctx, &source, &selection, &policy, None)
        )
        .await,
        Err(ImportedSourceError::Authentication(_))
    ));
    assert!(matches!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &source, &selection, &policy, None)
        )
        .await,
        Err(ImportedSourceError::Authentication(AuthError::Denied))
    ));
    assert!(matches!(
        inspect_imported_source_local(
            &f.issuer,
            "test-local-key",
            f.input(&ctx, &source, &selection, &policy, None)
        )
        .await,
        Err(ImportedSourceError::Authentication(AuthError::Denied))
    ));
    Ok(())
}

#[tokio::test]
async fn original_cancellation_and_deadline_and_invalid_selection_are_errors() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    let source = f.source(0, "request", "v1")?;
    corpus(&source, &[("file.any", Some("text"))]).await?;
    f.register(&source, &ctx)?;
    let policy = file_policy();
    let invalid = Request {
        pattern: "[".into(),
        is_regex: true,
        ..Request::default()
    };
    assert!(matches!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &source, &invalid, &policy, None)
        )
        .await,
        Err(ImportedSourceError::Inspection(
            SourceInspectionError::Selection(_)
        ))
    ));
    let expired = request_until(&cancel, Instant::now() + Duration::from_millis(1));
    std::thread::sleep(Duration::from_millis(3));
    assert!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&expired, &source, &invalid, &policy, None)
        )
        .await
        .is_err()
    );
    cancel.cancel();
    assert!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &source, &invalid, &policy, None)
        )
        .await
        .is_err()
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_and_wrong_entry_are_invalid_native_origins() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    let selection = Request::default();
    let policy = file_policy();
    let source = f.source(0, "symlink", "v1")?;
    fs::create_dir_all(source.directory().as_path().parent().unwrap())?;
    std::os::unix::fs::symlink(&f.projects[0].root, source.directory().as_path())?;
    f.register(&source, &ctx)?;
    assert!(matches!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &source, &selection, &policy, Some(0))
        )
        .await,
        Err(ImportedSourceError::InvalidOrigin)
    ));
    let graph_symlink = f.source(0, "graph-symlink", "v1")?;
    fs::create_dir_all(graph_symlink.directory().as_path())?;
    std::os::unix::fs::symlink(
        &f.projects[0].root,
        graph_symlink.directory().as_path().join("graph.icebug"),
    )?;
    f.register(&graph_symlink, &ctx)?;
    assert!(matches!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &graph_symlink, &selection, &policy, Some(0))
        )
        .await,
        Err(ImportedSourceError::InvalidOrigin)
    ));
    let source = f.source(0, "wrong-entry", "v1")?;
    fs::create_dir_all(source.directory().as_path().join("graph.icebug"))?;
    fs::write(
        source
            .directory()
            .as_path()
            .join("graph.icebug/source.lance"),
        "file",
    )?;
    f.register(&source, &ctx)?;
    assert!(matches!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &source, &selection, &policy, Some(0))
        )
        .await,
        Err(ImportedSourceError::InvalidOrigin)
    ));
    Ok(())
}

#[tokio::test]
async fn selectors_match_for_indexed_and_physical_files_and_misses_create_nothing() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    let indexed = f.source(0, "selectors/indexed", "v1")?;
    let physical = f.source(0, "selectors/physical", "v1")?;
    let text = "first\nneedle one\nthird\nneedle two\nlast";
    corpus(&indexed, &[("file.any", Some(text))]).await?;
    fs::create_dir_all(physical.directory().as_path().join("graph.icebug"))?;
    fs::write(f.projects[0].root.join("file.any"), text)?;
    f.register(&indexed, &ctx)?;
    f.register(&physical, &ctx)?;
    let policy = file_policy();
    for selection in [
        Request {
            head: 2,
            ..Request::default()
        },
        Request {
            tail: 2,
            ..Request::default()
        },
        Request {
            start_line: 2,
            end_line: 4,
            line_numbers: true,
            ..Request::default()
        },
        Request {
            pattern: "needle".into(),
            before: 1,
            after: 1,
            ..Request::default()
        },
        Request {
            pattern: "needle (one|two)".into(),
            is_regex: true,
            line_numbers: true,
            ..Request::default()
        },
        Request {
            pattern: "no match".into(),
            ..Request::default()
        },
    ] {
        let expected = graphit_core::text_slice::apply(text, &selection).unwrap();
        for (source, fallback) in [(&indexed, None), (&physical, Some(0))] {
            let result = inspect_imported_source_runtime(
                &f.issuer,
                &f.raw,
                f.input(&ctx, source, &selection, &policy, fallback),
            )
            .await?;
            assert_eq!(result.selection, expected);
        }
    }
    assert!(
        !physical
            .directory()
            .as_path()
            .join("graph.icebug/source.lance")
            .exists()
    );
    fs::write(f.projects[0].root.join("file.any"), [0xff, 0xfe])?;
    assert!(matches!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &physical, &Request::default(), &policy, Some(0))
        )
        .await,
        Err(ImportedSourceError::Inspection(
            SourceInspectionError::InvalidUtf8(_)
        ))
    ));
    let default = Request::default();
    let mut foreign = f.input(&ctx, &indexed, &default, &policy, None);
    foreign.installation.record = &f.projects[0].record;
    assert!(matches!(
        inspect_imported_source_runtime(&f.issuer, &f.raw, foreign).await,
        Err(ImportedSourceError::InvalidInstallation)
    ));
    Ok(())
}

#[tokio::test]
async fn multiple_installed_versions_select_exact_corpus_without_replacing_other_version()
-> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    let one = f.source(2, "same/artifact", "v1")?;
    let two = f.source(2, "same/artifact", "v2")?;
    corpus(&one, &[("file.any", Some("version one"))]).await?;
    corpus(&two, &[("file.any", Some("version two"))]).await?;
    f.register(&one, &ctx)?;
    f.register(&two, &ctx)?;
    let selection = Request::default();
    let policy = file_policy();
    for (source, expected) in [(&one, "version one"), (&two, "version two")] {
        let result = inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, source, &selection, &policy, None),
        )
        .await?;
        assert_eq!(result.origin, SourceOrigin::Indexed);
        assert_eq!(result.selection.source, expected);
    }
    let missing = f.source(2, "same/artifact", "v3")?;
    assert!(matches!(
        inspect_imported_source_runtime(
            &f.issuer,
            &f.raw,
            f.input(&ctx, &missing, &selection, &policy, None)
        )
        .await,
        Err(ImportedSourceError::InstallationNotFound)
    ));
    assert_eq!(f.registry.load()?.entries().count(), 2);
    Ok(())
}
