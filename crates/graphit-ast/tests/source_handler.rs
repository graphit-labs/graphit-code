use arrow_array::{ArrayRef, RecordBatch, StringArray};
use graphit_ast::{
    indexed_source::{IndexedSourceError, source_schema},
    source_handler::{
        SourceHandlerError, inspect_source_local, inspect_source_role, inspect_source_runtime,
    },
    source_index_locator::SourceIndexError,
    source_inspection::{SourceInspection, SourceInspectionError, SourceOrigin},
};
use graphit_auth::{
    AdminSessionPolicy, AuthError, IssuerAuthorityPolicy, IssuerLimits, Permission, ProjectCapture,
    RoleDefinition, RoleIssuer, ScopeCapture, ScopeInput, ServiceAdmissionLimits,
    ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::{
        AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
        ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, CapturedSourceReader, CopyLimits, CopyPolicy,
        RetainedBytesLimits, RetainedFileSet, ScopeBindingRecord, ScopeLimits, SnapshotPolicy,
        SourceReadLimits, StoreOwner,
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
use graphit_data::{lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore};
use graphit_fs::NodeKind;
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, NativeBinding, ProjectionLimits, SourceRef,
    native::{ContentLimits, Role},
    policy::RequestContext,
};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs, io,
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
    default: VersionContext,
    named: VersionContext,
}
struct Fixture {
    issuer: RoleIssuer,
    runtime: Arc<RuntimeKeyOwner>,
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
        let identity = InstallationIdentityStore::new(&paths)?.ensure(
            ctx.remaining()?.expect("finite fixture deadline"),
            ctx.cancellation(),
        )?;
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
            let default = VersionContext::select(&project, checkout.as_checkout(), "", None, "")?;
            let named =
                VersionContext::select(&project, checkout.as_checkout(), "feature", None, "")?;
            roots.push(root.clone());
            projects.push(ProjectFixture {
                root,
                scope,
                record,
                default,
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
            runtime,
            installation,
            record,
            projects,
            raw,
            _temp: temp,
        })
    }
    fn scope(&self, project: usize, named: bool) -> ScopeInput<'_> {
        let p = &self.projects[project];
        ScopeInput {
            installation: ScopeCapture {
                live: &self.installation,
                record: &self.record,
            },
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &p.scope,
                    record: &p.record,
                },
                version: if named { &p.named } else { &p.default },
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        }
    }
    fn corpus(&self, project: usize) -> PathBuf {
        let StoreOwner::Project { project_id } = self.projects[project].scope.owner() else {
            panic!("actual project owner")
        };
        self.installation
            .owner_root()
            .join("ast")
            .join(project_id.to_string())
            .join("source.lance")
    }
    async fn runtime<'a>(
        &'a self,
        ctx: &'a RequestContext<'a>,
        project: usize,
        named: bool,
        path: &Path,
        selection: &Request,
    ) -> Result<SourceInspection, SourceHandlerError> {
        inspect_source_runtime(
            &self.issuer,
            &self.raw,
            self.scope(project, named),
            path,
            selection,
            &file_policy(),
            read_limits(),
            path_limits(),
            index_limits(),
            ctx,
        )
        .await
    }
    async fn local<'a>(
        &'a self,
        ctx: &'a RequestContext<'a>,
        path: &Path,
    ) -> Result<SourceInspection, SourceHandlerError> {
        inspect_source_local(
            &self.issuer,
            "test-local-key",
            self.scope(0, false),
            path,
            &Request::default(),
            &file_policy(),
            read_limits(),
            path_limits(),
            index_limits(),
            ctx,
        )
        .await
    }
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
async fn publish(
    f: &Fixture,
    project: usize,
    rows: &[(&str, Option<&str>)],
) -> Result<LanceModuleStore, Box<dyn Error>> {
    let path = f.corpus(project);
    fs::create_dir_all(path.parent().unwrap())?;
    Ok(
        LanceModuleStore::create(path.to_str().unwrap(), source_schema(), vec![batch(rows)])
            .await?,
    )
}

#[tokio::test]
async fn runtime_and_local_indexed_empty_and_raw_ignore_physical_file_kind() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, true)?;
    let raw = "\u{feff}α\r\n\0🦀";
    fs::create_dir(f.projects[0].root.join("empty"))?;
    fs::write(f.projects[0].root.join("raw"), "physical differs")?;
    publish(&f, 0, &[("empty", Some("")), ("raw", Some(raw))]).await?;
    for (path, expected) in [("empty", ""), ("raw", raw)] {
        for result in [
            f.runtime(&ctx, 0, false, Path::new(path), &Request::default())
                .await?,
            f.local(&ctx, Path::new(path)).await?,
        ] {
            assert_eq!(result.origin, SourceOrigin::Indexed);
            assert_eq!(result.selection.source, expected);
        }
    }
    Ok(())
}

#[tokio::test]
async fn explicit_misses_read_raw_empty_unknown_extension_and_global_selectors() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    let raw = "\u{feff}α\r\nβ\0\nlast";
    for path in ["missing.unknown", "null"] {
        fs::write(f.projects[0].root.join(path), raw)?;
    }
    fs::write(f.projects[0].root.join("empty"), "")?;
    let absent = f
        .runtime(
            &ctx,
            0,
            false,
            Path::new("missing.unknown"),
            &Request::default(),
        )
        .await?;
    assert_eq!(absent.origin, SourceOrigin::Physical);
    assert_eq!(absent.selection.source, raw);
    assert!(!f.installation.owner_root().join("ast").exists());
    publish(&f, 0, &[("null", None), ("empty", None)]).await?;
    for path in ["missing.unknown", "null"] {
        let result = f
            .runtime(&ctx, 0, false, Path::new(path), &Request::default())
            .await?;
        assert_eq!(result.origin, SourceOrigin::Physical);
        assert_eq!(result.selection.source, raw);
    }
    assert_eq!(
        f.runtime(&ctx, 0, false, Path::new("empty"), &Request::default())
            .await?
            .selection
            .source,
        ""
    );
    for (selection, expected, start) in [
        (
            Request {
                head: 1,
                ..Request::default()
            },
            "\u{feff}α\r",
            1,
        ),
        (
            Request {
                tail: 1,
                ..Request::default()
            },
            "last",
            3,
        ),
        (
            Request {
                start_line: 2,
                end_line: 2,
                ..Request::default()
            },
            "β\0",
            2,
        ),
    ] {
        let result = f
            .runtime(&ctx, 0, false, Path::new("null"), &selection)
            .await?;
        assert_eq!(result.selection.source, expected);
        assert_eq!(result.selection.start_line, Some(start));
    }
    let result = f
        .runtime(
            &ctx,
            0,
            false,
            Path::new("null"),
            &Request {
                pattern: "β".into(),
                ..Request::default()
            },
        )
        .await?;
    assert_eq!(result.selection.matches[0].line_number, 2);
    assert_eq!(result.selection.matches[0].line, "β\0");
    Ok(())
}

#[tokio::test]
async fn project_and_named_version_selection_and_actual_sdk_failure_never_fallback() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    for p in &f.projects {
        fs::write(p.root.join("same"), "disk fallback must not appear")?;
    }
    publish(&f, 0, &[("same", Some("default A"))]).await?;
    publish(&f, 1, &[("same", Some("project B"))]).await?;
    let corpus = f.corpus(0);
    let store = LanceBranchStore::open(corpus.to_str().unwrap(), source_schema(), None).await?;
    let encoded = f.projects[0].named.lance_branch();
    store
        .capture_target(Some(&encoded))
        .await?
        .materialize()
        .await?
        .prepare_merge(
            vec![batch(&[("same", Some("named C"))])],
            vec!["path".into()],
        )
        .await?
        .commit()
        .await?;
    for (project, named, expected) in [
        (0, false, "default A"),
        (0, true, "named C"),
        (1, false, "project B"),
    ] {
        let result = f
            .runtime(&ctx, project, named, Path::new("same"), &Request::default())
            .await?;
        assert_eq!(result.origin, SourceOrigin::Indexed);
        assert_eq!(result.selection.source, expected);
    }
    let dataset = lance::Dataset::open(corpus.to_str().unwrap()).await?;
    let fragments = dataset.get_fragments();
    let data = fragments[0].metadata().files.first().unwrap();
    assert_eq!(data.base_id, None);
    fs::remove_file(corpus.join("data").join(&data.path))?;
    let error = f
        .runtime(&ctx, 0, false, Path::new("same"), &Request::default())
        .await
        .err()
        .unwrap();
    assert!(matches!(
        &error,
        SourceHandlerError::Index(SourceIndexError::Source(IndexedSourceError::Sdk(_)))
    ));
    assert!(error.source().is_some());
    Ok(())
}

#[tokio::test]
async fn ordinary_denial_and_wrong_or_absent_keys_precede_source_payload() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let denied = Fixture::new(&ctx, false, true)?;
    fs::write(
        denied.installation.owner_root().join("ast"),
        "corrupt locator",
    )?;
    for error in [
        denied
            .runtime(&ctx, 0, false, Path::new("../invalid"), &Request::default())
            .await
            .err()
            .unwrap(),
        denied
            .local(&ctx, Path::new("../invalid"))
            .await
            .err()
            .unwrap(),
    ] {
        assert!(matches!(
            error,
            SourceHandlerError::Authentication(AuthError::Denied)
        ));
    }
    let allowed = Fixture::new(&ctx, true, false)?;
    let bad = "incorrect-transport-input";
    let error = inspect_source_runtime(
        &allowed.issuer,
        bad,
        allowed.scope(0, false),
        Path::new("../invalid"),
        &Request::default(),
        &file_policy(),
        read_limits(),
        path_limits(),
        index_limits(),
        &ctx,
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(&error, SourceHandlerError::Authentication(_)));
    assert!(!error.to_string().contains(bad));
    assert!(matches!(
        allowed.local(&ctx, Path::new("../invalid")).await,
        Err(SourceHandlerError::Authentication(_))
    ));
    let local = Fixture::new(&ctx, true, true)?;
    let error = inspect_source_local(
        &local.issuer,
        bad,
        local.scope(0, false),
        Path::new("../invalid"),
        &Request::default(),
        &file_policy(),
        read_limits(),
        path_limits(),
        index_limits(),
        &ctx,
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(&error, SourceHandlerError::Authentication(_)));
    assert!(!error.to_string().contains(bad));
    Ok(())
}

#[tokio::test]
async fn actual_role_file_registration_permission_scope_and_revocation() -> TestResult {
    let cancel = Cancellation::default();
    let mut ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    let role_dir = f.projects[0].root.join("roles/scout");
    fs::create_dir_all(&role_dir)?;
    let agent =
        "---\nname: scout\ndescription: Source reader\nread_only: true\n---\n\nInspect source.\n";
    fs::write(role_dir.join("AGENT.md"), agent)?;
    let captured = CapturedSourceReader::capture_project(
        &f.projects[0].scope,
        Path::new("roles/scout/AGENT.md"),
        NodeKind::File,
        &file_policy(),
        read_limits(),
        ctx.snapshot(),
    )?;
    let file = captured.open_file(Path::new(""))?;
    let observed = file.read_range(0, usize::try_from(file.len())?)?;
    let parsed = ctx.parse_agent(observed.as_ref())?;
    assert_eq!(parsed.name(), "scout");
    assert!(parsed.read_only());
    assert!(!parsed.allows_shell());
    captured.revalidate_content()?;
    let projection = ProjectionLimits::new(128, 4096, 32, 128, 32)?;
    let StoreOwner::Project { project_id } = f.projects[0].scope.owner() else {
        panic!("project owner")
    };
    let source = SourceRef::local_link(
        NativePath::new(role_dir)?,
        *project_id,
        ArtifactType::Agent,
        ArtifactId::new("roles/scout", projection)?,
        ConcreteVersion::new("1.0.0", projection)?,
        projection,
    )?;
    // Explicit trusted fixture administration registers this policy. Reading and
    // parsing the physical Agent is not an automatic conversion into a grant.
    let permission = Permission::Read(Module::Ast);
    let definition = RoleDefinition::new(
        Role::Scout,
        &source,
        NativeBinding::KiroV3,
        &[permission],
        Sha256::digest(observed.as_ref()).into(),
        issuer_limits(),
    )?;
    let reference = definition.reference().clone();
    let caller = f.issuer.authenticate_runtime(&f.raw, &ctx)?;
    let session = f.issuer.bootstrap_admin(&caller)?;
    let parent = f.issuer.admin_parent(&caller, &session)?;
    f.issuer.register_definition(&parent, definition)?;
    let scope = f.scope(0, false);
    let run = f
        .issuer
        .start_run(&parent, &scope, Duration::from_secs(300))?;
    let bearer = f
        .issuer
        .issue(&parent, &run, &reference, &[permission], &scope)?;
    fs::write(f.projects[0].root.join("file"), "role physical")?;
    let result = inspect_source_role(
        &f.issuer,
        bearer.expose_for_transport(),
        &reference,
        scope,
        Path::new("file"),
        &Request::default(),
        &file_policy(),
        read_limits(),
        path_limits(),
        index_limits(),
        &ctx,
    )
    .await?;
    assert_eq!(result.origin, SourceOrigin::Physical);
    assert_eq!(result.selection.source, "role physical");
    assert!(matches!(
        inspect_source_role(
            &f.issuer,
            bearer.expose_for_transport(),
            &reference,
            f.scope(1, false),
            Path::new("file"),
            &Request::default(),
            &file_policy(),
            read_limits(),
            path_limits(),
            index_limits(),
            &ctx
        )
        .await,
        Err(SourceHandlerError::Authentication(AuthError::Denied))
    ));
    f.issuer.revoke(&parent, &bearer)?;
    fs::write(f.installation.owner_root().join("ast"), "wrong kind")?;
    assert!(matches!(
        inspect_source_role(
            &f.issuer,
            bearer.expose_for_transport(),
            &reference,
            scope,
            Path::new("file"),
            &Request::default(),
            &file_policy(),
            read_limits(),
            path_limits(),
            index_limits(),
            &ctx
        )
        .await,
        Err(SourceHandlerError::Authentication(AuthError::Denied))
    ));
    Ok(())
}

#[tokio::test]
async fn native_paths_selectors_original_cancellation_and_deadline_stay_errors() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    fs::write(f.projects[0].root.join("same"), "one\ntwo")?;
    for path in [
        "",
        "../same",
        "a/../same",
        "a//same",
        "a/./same",
        "same/",
        "a\0b",
    ] {
        let error = f
            .runtime(&ctx, 0, false, Path::new(path), &Request::default())
            .await
            .err()
            .unwrap();
        assert!(
            matches!(&error, SourceHandlerError::InvalidPath)
                || matches!(&error, SourceHandlerError::Operation(e) if e.kind() == io::ErrorKind::InvalidInput)
        );
    }
    assert!(
        f.runtime(
            &ctx,
            0,
            false,
            &f.projects[0].root.join("same"),
            &Request::default()
        )
        .await
        .is_err()
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(vec![0xff]));
        assert!(matches!(
            f.runtime(&ctx, 0, false, &path, &Request::default()).await,
            Err(SourceHandlerError::InvalidPath)
        ));
    }
    for selection in [
        Request {
            head: -1,
            ..Request::default()
        },
        Request {
            start_line: 2,
            end_line: 1,
            ..Request::default()
        },
        Request {
            pattern: "[".into(),
            is_regex: true,
            ..Request::default()
        },
    ] {
        assert!(matches!(
            f.runtime(&ctx, 0, false, Path::new("same"), &selection)
                .await,
            Err(SourceHandlerError::Inspection(
                SourceInspectionError::Selection(_)
            ))
        ));
    }
    let other_cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_millis(50);
    let expired = request_until(&other_cancel, deadline);
    tokio::time::sleep_until(deadline.into()).await;
    assert!(matches!(
        f.runtime(&expired, 0, false, Path::new("same"), &Request::default())
            .await,
        Err(SourceHandlerError::Authentication(AuthError::Request))
    ));
    cancel.cancel();
    assert!(matches!(
        f.runtime(&ctx, 0, false, Path::new("same"), &Request::default())
            .await,
        Err(SourceHandlerError::Authentication(AuthError::Request))
    ));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn index_and_physical_symlinks_and_stale_project_scope_never_fallback() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    fs::write(f.projects[0].root.join("file"), "fallback must not appear")?;
    let target = f._temp.path().join("external");
    fs::create_dir(&target)?;
    let ast = f.installation.owner_root().join("ast");
    std::os::unix::fs::symlink(&target, &ast)?;
    assert!(matches!(
        f.runtime(&ctx, 0, false, Path::new("file"), &Request::default())
            .await,
        Err(SourceHandlerError::Index(SourceIndexError::UnexpectedEntry))
    ));
    fs::remove_file(&ast)?;
    std::os::unix::fs::symlink(
        f.projects[0].root.join("file"),
        f.projects[0].root.join("link"),
    )?;
    let error = f
        .runtime(&ctx, 0, false, Path::new("link"), &Request::default())
        .await
        .err()
        .unwrap();
    assert!(matches!(&error, SourceHandlerError::Operation(_)));
    assert!(error.source().is_some());
    let moved = f.projects[0].root.with_extension("moved");
    fs::rename(&f.projects[0].root, moved)?;
    fs::create_dir(&f.projects[0].root)?;
    fs::write(f.projects[0].root.join("file"), "new unauthorized root")?;
    assert!(
        f.runtime(&ctx, 0, false, Path::new("file"), &Request::default())
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn physical_invalid_utf8_read_limits_and_runtime_revocation_preserve_causes() -> TestResult {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, true, false)?;
    fs::write(f.projects[0].root.join("bad"), [0xff, 0xfe])?;
    let error = f
        .runtime(&ctx, 0, false, Path::new("bad"), &Request::default())
        .await
        .err()
        .unwrap();
    assert!(matches!(
        &error,
        SourceHandlerError::Inspection(SourceInspectionError::InvalidUtf8(_))
    ));
    assert!(error.source().is_some());
    fs::write(f.projects[0].root.join("large"), "abc")?;
    let error = inspect_source_runtime(
        &f.issuer,
        &f.raw,
        f.scope(0, false),
        Path::new("large"),
        &Request::default(),
        &file_policy(),
        SourceReadLimits::new(1, 1, 1)?,
        path_limits(),
        index_limits(),
        &ctx,
    )
    .await
    .err()
    .unwrap();
    assert!(
        matches!(&error, SourceHandlerError::Inspection(SourceInspectionError::PhysicalRead(e))
        if e.kind() == io::ErrorKind::FileTooLarge)
    );
    assert!(error.source().is_some());
    let outcome = f.runtime.remove(ctx.snapshot());
    assert!(outcome.failure().is_none());
    assert!(matches!(
        f.runtime(&ctx, 0, false, Path::new("large"), &Request::default())
            .await,
        Err(SourceHandlerError::Authentication(_))
    ));
    Ok(())
}
