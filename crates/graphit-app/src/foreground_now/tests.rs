#[cfg(target_os = "linux")]
#[path = "../../tests/support/finite_cli.rs"]
mod finite_cli;
use super::*;
#[cfg(target_os = "linux")]
use finite_cli::FiniteOutput;
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, ServiceAdmissionLimits,
    ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::{
        AiService, AuthDocument, BrokerService, ProfileLimits, ProviderConfig, S3Location, S3Source,
    },
};
use graphit_core::{
    auth_document::AuthDocumentLimits,
    brand::Brand,
    config::ConfigSnapshot,
    installation_bootstrap::BootstrapLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::InstallationPaths,
    runtime_key::RuntimeKeyLimits,
    version_context::Checkout,
};
use graphit_daemon::auth_startup::StartupPolicy;
use std::time::Instant;

struct Fixture {
    owner: ForegroundNow,
    selected: (ProjectPaths, ProjectId, VersionContext),
    _directory: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let policy =
            HubResourcePolicy::resolve(&ConfigSnapshot::new(paths.brand(), []).unwrap()).unwrap();
        let stop = Cancellation::default();
        let original = policy.request_context(Instant::now(), &stop).unwrap();
        let source = || SourceAuthorityPolicy {
            ordinary: &[],
            administrative: &[],
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
            },
        };
        let profiles = ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 4,
            profiles: 4,
        };
        let scope = ScopeLimits::new(16, 4096, 128).unwrap();
        let auth = Arc::new(
            DaemonAuth::start(
                &paths,
                &[],
                StartupPolicy {
                    bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
                    runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
                    scope,
                    document: AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
                    profiles,
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
                                max_rules: 4,
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
                },
                &original,
            )
            .unwrap(),
        );
        let mut document = AuthDocument::new(profiles, &original).unwrap();
        document
            .add_provider(
                "now".into(),
                ProviderConfig {
                    kind: ProviderKind::Broker(BrokerService {
                        endpoint: "http://127.0.0.1:1".into(),
                        mcp_resource: None,
                        allow_anonymous: false,
                    }),
                    embedding: AiService::Broker,
                    rerank: AiService::Broker,
                    s3: S3Location {
                        bucket: None,
                        region: None,
                        endpoint: None,
                        prefix: None,
                        source: S3Source::BrokerScope,
                    },
                },
                document.revision(),
                1,
                &original,
            )
            .unwrap();
        let root = directory.path().join("project");
        std::fs::create_dir(&root).unwrap();
        let project = ProjectPaths::at(&root, &paths).unwrap();
        let id = ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &stop)
            .unwrap()
            .ensure(&ProjectMetadata::new("refresh-owner").unwrap())
            .unwrap()
            .id();
        let version = VersionContext::select(&project, Checkout::NonGit, "main", None, "").unwrap();
        let selected = (project, id, version);
        let owner = ForegroundNow::new(
            auth,
            Arc::new(Vec::new()),
            document.provider("now").unwrap().clone(),
            None,
            Settings {
                scope,
                native: NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
                policy,
                binding: NativeBinding::KiroV3,
                network: Limits {
                    request_body_bytes: 65536,
                    response_body_bytes: 65536,
                    url_bytes: 2048,
                    header_bytes: 16384,
                    header_count: 32,
                    max_in_flight: 4,
                    max_idle_per_host: 2,
                    idle_timeout: Duration::from_secs(5),
                    max_certificates: 2,
                    max_certificate_bytes: 8192,
                },
                allow_loopback_http: true,
            },
        )
        .unwrap();
        Self {
            owner,
            selected,
            _directory: directory,
        }
    }
    fn task(&mut self, result: io::Result<()>) {
        let stop = Cancellation::default();
        let handle = self.owner.jobs.spawn(async move { result });
        self.owner.projects.insert(
            handle.id(),
            (self.selected.1, self.selected.2.clone(), stop),
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let stop = Cancellation::default();
        let original = self
            .owner
            .plan
            .policy
            .request_context(Instant::now(), &stop)
            .unwrap();
        let removed = self.owner.plan.auth.remove_runtime(original.snapshot());
        assert!(removed.failure().is_none() && removed.cleanup_failure().is_none());
    }
}

#[tokio::test]
async fn consumed_last_refresh_event_remains_observable_without_a_publisher() {
    for error in [
        io::Error::other(ServiceUseError::NeedsRefresh),
        io::Error::other(NowPublisherError::from(ServiceUseError::NeedsRefresh)),
    ] {
        let mut f = Fixture::new();
        f.task(Err(error));
        let failure = f.owner.next().await.unwrap_err();
        assert!(ForegroundNow::needs_native_refresh(&failure));
        assert!(f.owner.projects.is_empty() && f.owner.jobs.is_empty());
        // Source reconciliation may have consumed the final join; the outer loop still sees it.
        let failure = tokio::time::timeout(Duration::from_millis(100), f.owner.next())
            .await
            .unwrap()
            .unwrap_err();
        assert!(ForegroundNow::needs_native_refresh(&failure));
        f.owner.shutdown().await.unwrap();
    }
}

#[test]
fn closed_and_deadline_notices_recover_without_retrying_local_or_authority_failures() {
    for code in [
        NowRemoteCode::InvalidData,
        NowRemoteCode::TooLarge,
        NowRemoteCode::Unauthenticated,
        NowRemoteCode::Forbidden,
        NowRemoteCode::Deadline,
        NowRemoteCode::AuthorizationUnavailable,
        NowRemoteCode::Closed,
        NowRemoteCode::Superseded,
        NowRemoteCode::Backpressure,
        NowRemoteCode::OutcomeUncertain,
        NowRemoteCode::NotRegistered,
        NowRemoteCode::OwnerConflict,
    ] {
        let error = io::Error::other(NowPublisherError::Remote(code));
        assert_eq!(
            reconnectable(&error),
            matches!(code, NowRemoteCode::Closed | NowRemoteCode::Deadline),
            "{code:?}"
        );
    }
    assert!(!reconnectable(&io::Error::other(
        NowPublisherError::Closed(1008)
    )));
    assert!(!reconnectable(&io::Error::other(
        ServiceUseError::LoginRequired
    )));
    assert!(!reconnectable(&io::Error::from(io::ErrorKind::Interrupted)));
    assert!(!reconnectable(&io::Error::from(io::ErrorKind::TimedOut)));
    assert!(!reconnectable(&io::Error::other(NowPublisherError::from(
        io::Error::from(io::ErrorKind::TimedOut)
    ))));
    assert!(!reconnectable(&io::Error::other(
        NowPublisherError::DifferentOriginal
    )));
    for error in [
        graphit_auth::AuthError::Denied,
        graphit_auth::AuthError::Expired,
        graphit_auth::AuthError::Request,
        graphit_auth::AuthError::State,
        graphit_auth::AuthError::Invalid,
    ] {
        let error = io::Error::other(NowPublisherError::from(ServiceUseError::Auth(error)));
        assert!(!reconnectable(&error));
        assert!(!ForegroundNow::needs_native_refresh(&error));
    }
    for error in [
        ServiceUseError::Auth(graphit_auth::AuthError::Io),
        ServiceUseError::Store(graphit_auth::store::AuthStoreError::Core(
            graphit_core::auth_document::AuthDocumentError::Io,
        )),
    ] {
        let error = io::Error::other(NowPublisherError::from(error));
        assert!(reconnectable(&error));
        assert!(!ForegroundNow::needs_native_refresh(&error));
    }
    let disguised = io::Error::other("NeedsRefresh private-path/private-token");
    assert!(!ForegroundNow::needs_native_refresh(&disguised));
}

#[cfg(target_os = "linux")]
#[test]
fn physical_admission_io_readmits_without_reusing_authority() {
    const CHILD: &str = "GRAPHIT_NOW_DESCRIPTOR_FAILURE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // Reproduce the actual OS failure only in this child, never by lowering
        // the developer's daemon or the shared test process descriptor limit.
        let inherited = rustix::process::getrlimit(rustix::process::Resource::Nofile);
        let directory = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("HOME", directory.path())
            .current_dir(directory.path())
            .args([
                "--exact",
                "foreground_now::tests::physical_admission_io_readmits_without_reusing_authority",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .finite_output()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated descriptor qualification failed: {output:?}"
        );
        assert_eq!(
            rustix::process::getrlimit(rustix::process::Resource::Nofile),
            inherited
        );
        return;
    }
    let mut limit = rustix::process::getrlimit(rustix::process::Resource::Nofile);
    limit.current = Some(limit.maximum.unwrap_or(128).min(128));
    rustix::process::setrlimit(rustix::process::Resource::Nofile, limit).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut f = Fixture::new();
        let stop = Cancellation::default();
        let original = f
            .owner
            .plan
            .policy
            .request_context(Instant::now(), &stop)
            .unwrap();
        let key = f
            .owner
            .plan
            .auth
            .runtime()
            .check(original.snapshot())
            .unwrap();
        let raw = std::str::from_utf8(key.secret(original.snapshot()).unwrap()).unwrap();
        let caller = f
            .owner
            .plan
            .auth
            .issuer()
            .authenticate_runtime(raw, &original)
            .unwrap();
        let mut held = Vec::new();
        let exhausted = loop {
            match std::fs::File::open("/dev/null") {
                Ok(file) => held.push(file),
                Err(error) => break error,
            }
        };
        assert_eq!(
            exhausted.raw_os_error(),
            Some(rustix::io::Errno::MFILE.raw_os_error())
        );
        assert_eq!(caller.check(), Err(graphit_auth::AuthError::Io));
        let (ready, mut readiness) = oneshot::channel();
        let mut publisher = Box::pin(run(
            Arc::clone(&f.owner.plan),
            f.selected.clone(),
            stop.clone(),
            ready,
        ));
        std::future::poll_fn(|context| {
            assert!(publisher.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        assert!(matches!(
            readiness.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        drop(held);
        caller.check().unwrap();
        // This existing fixture has no admitted Broker profile. A new attempt
        // must reach that real terminal denial, not report readiness or reuse
        // authority from the failed physical capture.
        let error = tokio::time::timeout(Duration::from_secs(3), publisher)
            .await
            .unwrap()
            .unwrap_err();
        assert!(!reconnectable(&error), "{error}");
        assert!(matches!(
            readiness.try_recv(),
            Err(oneshot::error::TryRecvError::Closed)
        ));
        assert!(!stop.is_cancelled());
        f.owner.shutdown().await.unwrap();
    });
}

#[tokio::test]
async fn actual_pre_welcome_reqwest_disconnect_is_recoverable_but_transport_is_not() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(socket.read_u8().await.unwrap());
        }
        socket.shutdown().await.unwrap();
    });
    let f = Fixture::new();
    let client = WebSocketClient::new(
        ClientPolicy {
            origin: &url,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        f.owner.plan.network,
        f.owner.plan.websocket,
    )
    .unwrap();
    let stop = Cancellation::default();
    let result = client
        .connect(
            &url,
            axum::http::HeaderMap::new(),
            graphit_http::client::NetworkRequestContext::new(
                Instant::now() + Duration::from_secs(5),
                &stop,
            ),
        )
        .await;
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("peer closed before upgrade"),
    };
    peer.await.unwrap();
    assert!(
        matches!(&error, WebSocketError::Sdk(cause) if cause.kind() == WebSocketFailureKind::Closed)
    );
    assert!(reconnectable(&io::Error::other(NowPublisherError::from(
        error
    ))));
    for source in [
        NetworkError::Transport,
        NetworkError::Cancelled,
        NetworkError::Deadline,
    ] {
        assert!(!reconnectable(&io::Error::other(
            NowPublisherError::Transport(WebSocketError::Network(source))
        )));
    }
}

#[tokio::test]
async fn refreshed_publishers_resume_in_background_and_keep_terminal_errors_observable() {
    let mut f = Fixture::new();
    // Rotation has already joined successfully; startup no longer owns admission.
    f.owner.initial_projects.take();
    let desired = Arc::new(vec![f.selected.clone()]);
    let stop = Cancellation::default();
    for _ in 0..2 {
        tokio::time::timeout(
            Duration::from_secs(1),
            f.owner.refresh_native(Arc::clone(&desired), &stop),
        )
        .await
        .expect("readiness must not hold the service owner after rotation")
        .unwrap();
        assert_eq!(f.owner.jobs.len(), 1);
        assert_eq!(f.owner.projects.len(), 1);
    }
    // This fixture has no authenticated Broker profile. Admission must still
    // fail through the normal owner observation, rather than becoming ready.
    tokio::time::timeout(Duration::from_secs(1), f.owner.next())
        .await
        .unwrap()
        .expect_err("background admission errors remain terminal");
    assert!(f.owner.jobs.is_empty() && f.owner.projects.is_empty());
    stop.cancel();
    let error = f.owner.refresh_native(desired, &stop).await.unwrap_err();
    assert!(matches!(
        error
            .get_ref()
            .and_then(|cause| cause.downcast_ref::<graphit_hub::policy::RequestError>()),
        Some(graphit_hub::policy::RequestError::Content(
            graphit_hub::native::ContentError::Cancelled
        ))
    ));
    assert!(f.owner.jobs.is_empty() && f.owner.projects.is_empty());
    f.owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn refresh_quiesces_siblings_but_preserves_their_terminal_failure() {
    let mut f = Fixture::new();
    f.owner.refresh_required = true;
    f.task(Err(io::Error::other(NowPublisherError::Remote(
        graphit_daemon::now_subscription::NowRemoteCode::Superseded,
    ))));
    let stop = Cancellation::default();
    let failure = f
        .owner
        .refresh_native(Arc::new(Vec::new()), &stop)
        .await
        .unwrap_err();
    assert!(matches!(
        failure
            .get_ref()
            .and_then(|e| e.downcast_ref::<NowPublisherError>()),
        Some(NowPublisherError::Remote(
            graphit_daemon::now_subscription::NowRemoteCode::Superseded
        ))
    ));
    assert!(!ForegroundNow::needs_native_refresh(&failure));
    assert!(f.owner.refresh.is_none() && f.owner.jobs.is_empty() && f.owner.projects.is_empty());
    assert!(!f.owner.stop.is_cancelled());
    f.owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn inventory_refresh_requires_the_exact_unsent_auth_admission() {
    use graphit_http::client::NetworkEffect;
    let mut f = Fixture::new();
    for effect in [NetworkEffect::MayHaveApplied, NetworkEffect::NotSent] {
        let error = io::Error::other(OwnInventoryPublicationFailure::Admission {
            source: InventoryAdmissionFailure::Auth(ServiceUseError::NeedsRefresh),
            effect,
        });
        assert_eq!(
            f.owner.require_inventory_native_refresh(&error),
            effect == NetworkEffect::NotSent
        );
        assert_eq!(f.owner.refresh_required, effect == NetworkEffect::NotSent);
    }
    f.owner.refresh_required = false;
    let error = io::Error::other(ServiceUseError::NeedsRefresh);
    assert!(!f.owner.require_inventory_native_refresh(&error));
    assert!(!f.owner.refresh_required);
    for effect in [NetworkEffect::MayHaveApplied, NetworkEffect::NotSent] {
        let error = io::Error::other(OwnInventoryPublicationFailure::Admission {
            source: InventoryAdmissionFailure::Auth(ServiceUseError::Auth(
                graphit_auth::AuthError::Io,
            )),
            effect,
        });
        assert_eq!(reconnectable(&error), effect == NetworkEffect::NotSent);
        assert!(!f.owner.require_inventory_native_refresh(&error));
    }
    f.owner.shutdown().await.unwrap();
}
