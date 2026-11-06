use arrow_array::{ArrayRef, RecordBatch, StringArray};
use graphit_app::source_server::{SourcePolicy, SourceServer};
use graphit_ast::indexed_source::source_schema;
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ServiceAdmissionLimits,
    ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy, profile::ProfileLimits,
};
use graphit_core::{
    artifact_store::{
        CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet, ScopeLimits, SnapshotPolicy,
        SourceReadLimits,
    },
    auth_document::AuthDocumentLimits,
    brand::Brand,
    config::ConfigSnapshot,
    file_lock::Cancellation,
    git_checkout::GitCheckout,
    installation_bootstrap::BootstrapLimits,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectId, ProjectIdentityStore, ProjectMetadata},
    project_paths::InstallationPaths,
    project_paths::ProjectPaths,
    runtime_key::RuntimeKeyLimits,
    tcp_listener::{self, Options, PortSelection},
    version_context::VersionContext,
};
use graphit_daemon::{
    auth_startup::{DaemonAuth, StartupPolicy},
    source_discovery::{self, DiscoveryError, DiscoveryRecord},
};
use graphit_data::lance_module_store::LanceModuleStore;
use graphit_hub::{
    NativeBinding,
    native::ContentLimits,
    policy::{HubResourcePolicy, RequestContext},
    registry::HubRegistry,
};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
pub fn request(cancel: &Cancellation) -> RequestContext<'_> {
    request_until(cancel, Instant::now() + Duration::from_secs(30))
}
pub fn request_until(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(content(), cancel, deadline).unwrap()
}
fn scopes() -> ScopeLimits {
    ScopeLimits::new(16, 4096, 128).unwrap()
}
fn documents() -> AuthDocumentLimits {
    AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn profiles() -> ProfileLimits {
    ProfileLimits {
        bytes: 65536,
        nodes: 4096,
        scalar_bytes: 4096,
        collection_entries: 64,
        providers: 16,
        profiles: 16,
    }
}
fn policy(read: bool) -> StartupPolicy {
    let permissions = if read {
        vec![Permission::Read(Module::Ast)]
    } else {
        vec![]
    };
    let source = || SourceAuthorityPolicy {
        ordinary: &permissions,
        administrative: &[],
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    StartupPolicy {
        bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
        runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
        scope: scopes(),
        document: documents(),
        profiles: profiles(),
        authority: IssuerAuthorityPolicy::new(
            source(),
            source(),
            IssuerLimits {
                max_permissions: 64,
                max_scope_bytes: 65536,
                max_parent_bearer_bytes: 1024,
            },
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 8192,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap(),
        ttl: TtlPolicy {
            ttl: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
            refresh_margin: Duration::from_secs(120),
        },
    }
}

pub struct Fixture {
    auth: Arc<DaemonAuth>,
    pub paths: InstallationPaths,
    pub projects: Arc<Vec<(ProjectPaths, ProjectId, VersionContext)>>,
    _directory: tempfile::TempDir,
}
impl Fixture {
    #[allow(
        dead_code,
        reason = "Used by the proxy journey; this fixture is also compiled by the local client tests"
    )]
    pub async fn imported(&self, name: &str, text: &str) {
        use graphit_core::native_path::NativePath;
        use graphit_hub::{
            ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
            registry::{Installation, InstalledArtifact, InstalledBackend, InstalledPayload},
        };
        let registry = self.registry();
        let limits = registry.limits().projection();
        let source = SourceRef::local_link(
            NativePath::new(self._directory.path().join("imported").join(name)).unwrap(),
            self.projects[0].1,
            ArtifactType::Ast,
            ArtifactId::new(name, limits).unwrap(),
            ConcreteVersion::new("release-1", limits).unwrap(),
            limits,
        )
        .unwrap();
        fs::create_dir_all(source.directory().as_path().join("graph.icebug")).unwrap();
        let columns: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(vec!["same"])),
            Arc::new(StringArray::from(vec![Some(text)])),
        ];
        LanceModuleStore::create(
            source
                .directory()
                .as_path()
                .join("graph.icebug")
                .join("source.lance")
                .to_str()
                .unwrap(),
            source_schema(),
            vec![RecordBatch::try_new(source_schema(), columns).unwrap()],
        )
        .await
        .unwrap();
        registry
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
            .put_metadata(InstalledArtifact {
                consumer: graphit_core::artifact_store::PublicationSlot::Scope,
                artifact_type: ArtifactType::Ast,
                id: source.id().clone(),
                installations: vec![Installation {
                    target: InstallationTarget::GlobalNative,
                    original_source: source,
                    alias: None,
                    entry_name: None,
                    display: None,
                    backend: InstalledBackend::Native,
                    payload: InstalledPayload::Original {
                        selection_binding: None,
                    },
                }],
            })
            .unwrap();
    }
    pub fn new() -> Self {
        Self::in_directory(tempfile::tempdir().unwrap())
    }
    pub fn in_directory(directory: tempfile::TempDir) -> Self {
        let paths = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let trusted = ["first", "second"].map(|name| {
            let root = directory.path().join(name);
            fs::create_dir(&root).unwrap();
            ProjectPaths::at(&root, &paths).unwrap()
        });
        let auth = Arc::new(DaemonAuth::start(&paths, &trusted, policy(true), &ctx).unwrap());
        let mut projects = Vec::new();
        for (name, project) in ["first", "second"].into_iter().zip(trusted) {
            let root = project.root();
            let identity = ProjectIdentityStore::at(&project)
                .writer(Duration::from_secs(5), &cancel)
                .unwrap()
                .ensure(&ProjectMetadata::new(name).unwrap())
                .unwrap();
            let checkout = GitCheckout::inspect(root, Duration::from_secs(5), &cancel).unwrap();
            // Fixture-only setup observes actual checkout once. Source reads use retained facts.
            for explicit in ["", "feature"] {
                let version =
                    VersionContext::select(&project, checkout.as_checkout(), explicit, None, "")
                        .unwrap();
                projects.push((project.clone(), identity.id(), version));
            }
        }
        Self {
            auth,
            paths,
            projects: Arc::new(projects),
            _directory: directory,
        }
    }
    fn registry(&self) -> HubRegistry {
        let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
            self.auth.installation().live.owner()
        else {
            unreachable!()
        };
        let config = ConfigSnapshot::new(self.auth.installation().live.brand(), []).unwrap();
        HubRegistry::global(
            &self.paths,
            installation_id,
            HubResourcePolicy::resolve(&config).unwrap().hub(),
        )
        .unwrap()
    }
    fn source_policy(&self, timeout: Duration) -> SourcePolicy {
        let limits = RetainedBytesLimits::new(65536, 32, 4096).unwrap();
        let config = ConfigSnapshot::new(self.paths.brand(), []).unwrap();
        let original = graphit_core::file_io::SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        SourcePolicy {
            graph_query: graphit_ast::icebug_query::QueryOptions {
                buffer_pool_bytes: 128 * 1024 * 1024,
                query_threads: 2,
                max_rows: 64,
                max_bytes: 65536,
                max_depth: 32,
            },
            installation: self.paths.clone(),
            native_install: graphit_app::native_install_http::NativeInstallPolicy::capture(
                &config, &original,
            )
            .unwrap(),
            content: ContentLimits {
                max_input_bytes: 65536,
                max_output_bytes: 65536,
                max_depth: 32,
                max_nodes: 4096,
                max_scalar_bytes: 4096,
                max_collection_entries: 64,
            },
            timeout,
            scope: scopes(),
            file: SnapshotPolicy::new(
                CopyPolicy::RejectFileLinks,
                CopyLimits::new(1048576, 128, 65536, 8).unwrap(),
                RetainedFileSet::new([], limits).unwrap(),
                limits,
            )
            .unwrap(),
            read: SourceReadLimits::new(65536, 4096, 1048576).unwrap(),
            path: limits,
            index: NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
            binding: NativeBinding::KiroV3,
            registry: self.registry(),
        }
    }
    fn corpus(&self, project: usize) -> PathBuf {
        self.projects[project]
            .0
            .project_runtime(self.projects[project].1, &["ast", "source.lance"])
            .unwrap()
    }
    pub async fn publish(&self, project: usize, rows: &[(&str, Option<&str>)]) -> LanceModuleStore {
        let path = self.corpus(project);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let columns: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(
                rows.iter().map(|r| r.0).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                rows.iter().map(|r| r.1).collect::<Vec<_>>(),
            )),
        ];
        let batch = RecordBatch::try_new(source_schema(), columns).unwrap();
        LanceModuleStore::create(path.to_str().unwrap(), source_schema(), vec![batch])
            .await
            .unwrap()
    }
    pub fn file(&self, project: usize, path: &str, content: &str) {
        fs::write(self.projects[project].0.root().join(path), content).unwrap();
    }
    fn server(&self, timeout: Duration, stop: CancellationToken) -> SourceServer {
        SourceServer::new(
            Arc::clone(&self.auth),
            Arc::clone(&self.projects),
            0,
            self.source_policy(timeout),
            stop,
        )
        .unwrap()
    }
    pub async fn start(&self) -> Endpoint {
        let listener = tcp_listener::reserve(
            Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &Cancellation::default(),
        )
        .unwrap();
        let address = listener.address();
        let stop = CancellationToken::new();
        let handler = self.server(Duration::from_secs(30), stop.clone());
        let cancel = Cancellation::default();
        let original = request(&cancel);
        let discovery = source_discovery::publish_discovery(
            &self.auth,
            &listener,
            key_limits(),
            original.snapshot(),
        )
        .unwrap();
        let running = handler.clone();
        let task = tokio::spawn(async move { SourceServer::serve(&running, listener).await });
        Endpoint {
            auth: Arc::clone(&self.auth),
            discovery,
            stop,
            task,
            address,
        }
    }
}
pub struct Endpoint {
    auth: Arc<DaemonAuth>,
    discovery: DiscoveryRecord,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
    pub address: std::net::SocketAddr,
}
impl Endpoint {
    pub async fn stop(self) -> Result<bool, DiscoveryError> {
        self.stop.cancel();
        self.task
            .await
            .unwrap()
            .map_err(DiscoveryError::Operation)?;
        let cancel = Cancellation::default();
        let original = request(&cancel);
        source_discovery::remove_discovery(
            &self.auth,
            &self.discovery,
            key_limits(),
            original.snapshot(),
        )
    }
}
pub fn key_limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
pub fn content() -> ContentLimits {
    ContentLimits {
        max_input_bytes: 65536,
        max_output_bytes: 65536,
        max_depth: 32,
        max_nodes: 4096,
        max_scalar_bytes: 4096,
        max_collection_entries: 64,
    }
}

impl Fixture {
    pub async fn publish_named(
        &self,
        project: usize,
        rows: &[(&str, Option<&str>)],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let corpus = self.corpus(project);
        let branch = self.projects[project].2.lance_branch();
        assert!(!branch.is_empty());
        let columns: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(
                rows.iter().map(|r| r.0).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                rows.iter().map(|r| r.1).collect::<Vec<_>>(),
            )),
        ];
        let batch = RecordBatch::try_new(source_schema(), columns)?;
        let store = graphit_data::lance_branch_store::LanceBranchStore::open(
            corpus.to_str().unwrap(),
            source_schema(),
            None,
        )
        .await?;
        store
            .capture_target(Some(&branch))
            .await?
            .materialize()
            .await?
            .prepare_merge(vec![batch], vec!["path".into()])
            .await?
            .commit()
            .await?;
        Ok(())
    }

    pub async fn advertised(
        &self,
    ) -> (Endpoint, graphit_daemon::source_discovery::DiscoveryRecord) {
        let endpoint = self.start().await;
        let record = endpoint.discovery.clone();
        (endpoint, record)
    }

    pub async fn advertised_current(
        &self,
        current: usize,
    ) -> (Endpoint, graphit_daemon::source_discovery::DiscoveryRecord) {
        let cancel = Cancellation::default();
        let original = request(&cancel);
        let listener = tcp_listener::reserve(
            Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &cancel,
        )
        .unwrap();
        let address = listener.address();
        let stop = CancellationToken::new();
        let handler = SourceServer::new(
            Arc::clone(&self.auth),
            Arc::clone(&self.projects),
            current,
            self.source_policy(Duration::from_secs(30)),
            stop.clone(),
        )
        .unwrap();
        let record = graphit_daemon::source_discovery::publish_discovery(
            &self.auth,
            &listener,
            key_limits(),
            original.snapshot(),
        )
        .unwrap();
        let task = tokio::spawn(async move { SourceServer::serve(&handler, listener).await });
        (
            Endpoint {
                auth: Arc::clone(&self.auth),
                discovery: record.clone(),
                stop,
                task,
                address,
            },
            record,
        )
    }

    pub fn retract(&self, record: &graphit_daemon::source_discovery::DiscoveryRecord) {
        let cancel = Cancellation::default();
        let original = request(&cancel);
        assert!(
            graphit_daemon::source_discovery::remove_discovery(
                &self.auth,
                record,
                key_limits(),
                original.snapshot(),
            )
            .unwrap()
        );
        assert!(self.auth.runtime().check(original.snapshot()).is_ok());
    }
}
