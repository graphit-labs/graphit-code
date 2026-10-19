use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ServiceAdmissionLimits,
    ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy, profile::ProfileLimits,
};
use graphit_core::{
    artifact_store::{ScopeContext, ScopeLimits},
    auth_document::AuthDocumentLimits,
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::{Cancellation, FileLock},
    installation_bootstrap::BootstrapLimits,
    module_settings::Module,
    native_document_file::{
        CapturedDocumentDirectory, NativeDocumentFileLimits, ObservationChanged,
    },
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyEffect, RuntimeKeyLimits},
    tcp_listener::{self, BoundListener, Options, PortSelection},
};
use graphit_daemon::{
    auth_startup::{DaemonAuth, StartupPolicy},
    source_discovery::{DiscoveryError, publish_discovery, read_discovery, remove_discovery},
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use std::{
    error::Error,
    fs, io,
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    time::{Duration, Instant},
};
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    request_until(cancel, Instant::now() + Duration::from_secs(30))
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
fn policy() -> StartupPolicy {
    let permissions = [Permission::Read(Module::Ast)];
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
struct Fixture {
    _directory: tempfile::TempDir,
    paths: InstallationPaths,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("installation");
        let paths =
            InstallationPaths::resolve(Brand::default(), directory.path(), None, Some(&root))
                .unwrap();
        Self {
            _directory: directory,
            paths,
        }
    }
    fn key_path(&self) -> PathBuf {
        self.paths.global_path(&["daemon", "mcp.key"]).unwrap()
    }
    fn lock_path(&self) -> PathBuf {
        self.paths.global_path(&["daemon.pid"]).unwrap()
    }
}

fn limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(4096, 65536, 256, 65536).unwrap()
}
fn listener(address: Ipv4Addr, cancel: &Cancellation) -> BoundListener {
    tcp_listener::reserve(
        Options {
            address: IpAddr::V4(address),
            ports: PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        cancel,
    )
    .unwrap()
}
fn discovery(fixture: &Fixture) -> PathBuf {
    fixture.paths.global_path(&["daemon", "mcp.json"]).unwrap()
}
fn key(auth: &DaemonAuth, original: &SnapshotContext) -> Vec<u8> {
    auth.runtime()
        .check(original)
        .unwrap()
        .secret(original)
        .unwrap()
        .to_vec()
}
fn cleanup(auth: &DaemonAuth, original: &SnapshotContext) {
    let outcome = auth.remove_runtime(original);
    assert_eq!(outcome.effects().key, RuntimeKeyEffect::Applied);
    assert!(outcome.failure().is_none());
    assert!(outcome.cleanup_failure().is_none());
}

#[test]
fn absent_discovery_is_readonly_and_existing_invalid_directories_are_errors() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    assert_eq!(
        read_discovery(&fixture.paths, limits(), ctx.snapshot()).unwrap(),
        None
    );
    assert!(!fixture.paths.root().exists());
    fs::create_dir_all(fixture.paths.root()).unwrap();
    fs::write(
        fixture.paths.global_path(&["daemon"]).unwrap(),
        b"wrong directory kind",
    )
    .unwrap();
    assert!(read_discovery(&fixture.paths, limits(), ctx.snapshot()).is_err());
    assert!(!fixture.key_path().exists());
}

#[test]
fn missing_captured_checkout_preserves_owned_discovery_publication_and_cleanup() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let root = fixture._directory.path().join("captured-checkout");
    fs::create_dir(&root).unwrap();
    let project = ProjectPaths::at(&root, &fixture.paths).unwrap();
    let auth = DaemonAuth::start(
        &fixture.paths,
        std::slice::from_ref(&project),
        policy(),
        &ctx,
    )
    .unwrap();
    let original_key = key(&auth, ctx.snapshot());
    let socket = listener(Ipv4Addr::LOCALHOST, &cancel);
    let initial = publish_discovery(&auth, &socket, limits(), ctx.snapshot()).unwrap();
    fs::remove_dir_all(&root).unwrap();
    let context = ScopeContext::new(
        ctx.deadline().expect("finite fixture deadline"),
        cancel.clone(),
    );
    assert!(
        auth.installation()
            .live
            .revalidate_bounded(&context)
            .is_err()
    );
    assert!(remove_discovery(&auth, &initial, limits(), ctx.snapshot()).unwrap());
    assert!(!discovery(&fixture).exists());
    assert_eq!(key(&auth, ctx.snapshot()), original_key);
    let record = publish_discovery(&auth, &socket, limits(), ctx.snapshot()).unwrap();
    assert_eq!(record, initial);
    assert!(
        auth.installation()
            .live
            .revalidate_bounded(&context)
            .is_err()
    );
    let other = listener(Ipv4Addr::LOCALHOST, &cancel);
    let replacement = publish_discovery(&auth, &other, limits(), ctx.snapshot()).unwrap();
    assert!(!remove_discovery(&auth, &record, limits(), ctx.snapshot()).unwrap());
    assert_eq!(
        read_discovery(&fixture.paths, limits(), ctx.snapshot()).unwrap(),
        Some(replacement.clone())
    );
    assert!(remove_discovery(&auth, &replacement, limits(), ctx.snapshot()).unwrap());
    assert!(!remove_discovery(&auth, &replacement, limits(), ctx.snapshot()).unwrap());
    assert_eq!(key(&auth, ctx.snapshot()), original_key);
    assert!(!root.exists());
    assert_eq!(
        FileLock::try_acquire(fixture.lock_path())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    cleanup(&auth, ctx.snapshot());
}

#[test]
fn actual_listener_publication_is_private_and_does_not_rotate_the_live_key() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let auth = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    let before = key(&auth, ctx.snapshot());
    let socket = listener(Ipv4Addr::LOCALHOST, &cancel);
    let record = publish_discovery(&auth, &socket, limits(), ctx.snapshot()).unwrap();
    assert_eq!(record.pid(), std::process::id());
    assert_eq!(record.address(), socket.address());
    assert_eq!(
        read_discovery(&fixture.paths, limits(), ctx.snapshot())
            .unwrap()
            .as_ref(),
        Some(&record)
    );
    let bytes = fs::read(discovery(&fixture)).unwrap();
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document.as_object().unwrap().len(), 2);
    assert!(!bytes.windows(before.len()).any(|window| window == before));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(discovery(&fixture))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert!(DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).is_err());
    assert_eq!(key(&auth, ctx.snapshot()), before);
    assert!(remove_discovery(&auth, &record, limits(), ctx.snapshot()).unwrap());
    assert!(!remove_discovery(&auth, &record, limits(), ctx.snapshot()).unwrap());
    assert_eq!(key(&auth, ctx.snapshot()), before);
    assert_eq!(
        FileLock::try_acquire(fixture.lock_path())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    cleanup(&auth, ctx.snapshot());
    drop(auth);
    assert!(fixture.lock_path().exists());
    let restarted = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    assert_ne!(key(&restarted, ctx.snapshot()), before);
    let next = publish_discovery(&restarted, &socket, limits(), ctx.snapshot()).unwrap();
    assert!(remove_discovery(&restarted, &next, limits(), ctx.snapshot()).unwrap());
    cleanup(&restarted, ctx.snapshot());
}

#[test]
fn replacement_is_preserved_and_a_new_publication_is_complete() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let auth = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    let first = listener(Ipv4Addr::LOCALHOST, &cancel);
    let second = listener(Ipv4Addr::LOCALHOST, &cancel);
    let old = publish_discovery(&auth, &first, limits(), ctx.snapshot()).unwrap();
    let current = publish_discovery(&auth, &second, limits(), ctx.snapshot()).unwrap();
    assert_ne!(old.address(), current.address());
    assert!(!remove_discovery(&auth, &old, limits(), ctx.snapshot()).unwrap());
    assert_eq!(
        read_discovery(&fixture.paths, limits(), ctx.snapshot()).unwrap(),
        Some(current.clone())
    );
    let foreign = br#"{"pid":1,"address":"127.0.0.1:32100"}"#;
    fs::write(discovery(&fixture), foreign).unwrap();
    assert!(!remove_discovery(&auth, &current, limits(), ctx.snapshot()).unwrap());
    assert_eq!(fs::read(discovery(&fixture)).unwrap(), foreign);
    cleanup(&auth, ctx.snapshot());
}

#[test]
fn native_observation_changes_are_typed_after_real_birth_replacement_and_retirement() {
    fn changed(error: io::Error) {
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(
            error
                .get_ref()
                .is_some_and(|cause| cause.is::<ObservationChanged>())
        );
        let classified = DiscoveryError::from(error);
        assert!(matches!(&classified, DiscoveryError::ObservationChanged(_)));
        assert!(classified.source().is_some());
    }
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let auth = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    let directory = CapturedDocumentDirectory::capture(
        &fixture.paths.global_path(&["daemon"]).unwrap(),
        limits(),
        ctx.snapshot(),
    )
    .unwrap();
    let name = std::ffi::OsStr::new("mcp.json");
    let absent = directory.read(name, ctx.snapshot()).unwrap();
    assert!(
        read_discovery(&fixture.paths, limits(), ctx.snapshot())
            .unwrap()
            .is_none()
    );
    let first = listener(Ipv4Addr::LOCALHOST, &cancel);
    let second = listener(Ipv4Addr::LOCALHOST, &cancel);
    let old = publish_discovery(&auth, &first, limits(), ctx.snapshot()).unwrap();
    changed(
        directory
            .revalidate_observation(&absent, ctx.snapshot())
            .unwrap_err(),
    );
    let previous = directory.read(name, ctx.snapshot()).unwrap();
    let current = publish_discovery(&auth, &second, limits(), ctx.snapshot()).unwrap();
    assert_ne!(old.address(), current.address());
    assert_eq!(
        read_discovery(&fixture.paths, limits(), ctx.snapshot()).unwrap(),
        Some(current.clone())
    );
    changed(
        directory
            .revalidate_observation(&previous, ctx.snapshot())
            .unwrap_err(),
    );
    let present = directory.read(name, ctx.snapshot()).unwrap();
    assert!(remove_discovery(&auth, &current, limits(), ctx.snapshot()).unwrap());
    changed(
        directory
            .revalidate_observation(&present, ctx.snapshot())
            .unwrap_err(),
    );
    assert!(
        read_discovery(&fixture.paths, limits(), ctx.snapshot())
            .unwrap()
            .is_none()
    );
    let restored = publish_discovery(&auth, &second, limits(), ctx.snapshot()).unwrap();
    let alias = fixture._directory.path().join("discovery-hardlink");
    fs::hard_link(discovery(&fixture), &alias).unwrap();
    let error = read_discovery(&fixture.paths, limits(), ctx.snapshot()).unwrap_err();
    assert!(matches!(&error, DiscoveryError::Operation(_)));
    assert!(!matches!(&error, DiscoveryError::ObservationChanged(_)));
    fs::remove_file(alias).unwrap();
    assert!(remove_discovery(&auth, &restored, limits(), ctx.snapshot()).unwrap());
    cleanup(&auth, ctx.snapshot());
}

#[test]
fn malformed_records_keep_typed_errors_without_echoing_payloads() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let auth = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    for body in [
        r#"{"pid":0,"address":"127.0.0.1:1"}"#,
        r#"{"pid":1,"address":"127.0.0.1:0"}"#,
        r#"{"pid":1,"address":"192.0.2.1:1"}"#,
        r#"{"pid":1,"pid":2,"address":"127.0.0.1:1"}"#,
        r#"{"pid":1,"address":"127.0.0.1:1","SECRET_CANARY":1}"#,
        r#"{"pid":"SECRET_CANARY","address":"127.0.0.1:1"}"#,
        "not JSON SECRET_CANARY",
    ] {
        fs::write(discovery(&fixture), body).unwrap();
        let error = read_discovery(&fixture.paths, limits(), ctx.snapshot()).unwrap_err();
        assert!(!format!("{error:?}").contains("SECRET_CANARY"));
        assert!(matches!(
            &error,
            DiscoveryError::Json(_) | DiscoveryError::InvalidRecord
        ));
        if matches!(error, DiscoveryError::Json(_)) {
            assert!(error.source().is_some());
        }
        assert_eq!(fs::read_to_string(discovery(&fixture)).unwrap(), body);
    }
    fs::remove_file(discovery(&fixture)).unwrap();
    fs::create_dir(discovery(&fixture)).unwrap();
    assert!(read_discovery(&fixture.paths, limits(), ctx.snapshot()).is_err());
    cleanup(&auth, ctx.snapshot());
}

#[test]
fn cancelled_and_expired_publications_have_no_effect() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let auth = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    let socket = listener(Ipv4Addr::LOCALHOST, &cancel);
    let record = publish_discovery(&auth, &socket, limits(), ctx.snapshot()).unwrap();
    let bytes = fs::read(discovery(&fixture)).unwrap();
    let stopped = Cancellation::default();
    let denied = request(&stopped);
    stopped.cancel();
    for original in [
        denied.snapshot().as_ref(),
        &SnapshotContext::new(Instant::now(), Cancellation::default()),
    ] {
        assert!(matches!(
            read_discovery(&fixture.paths, limits(), original),
            Err(DiscoveryError::Operation(_))
        ));
        assert!(publish_discovery(&auth, &socket, limits(), original).is_err());
        assert!(remove_discovery(&auth, &record, limits(), original).is_err());
        assert_eq!(fs::read(discovery(&fixture)).unwrap(), bytes);
    }
    assert!(remove_discovery(&auth, &record, limits(), ctx.snapshot()).unwrap());
    cleanup(&auth, ctx.snapshot());
}

#[cfg(unix)]
#[test]
fn symlink_and_permission_errors_are_not_missing_or_overwritten() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let auth = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    let socket = listener(Ipv4Addr::LOCALHOST, &cancel);
    let target = fixture._directory.path().join("outside.json");
    fs::write(&target, br#"{"pid":1,"address":"127.0.0.1:1"}"#).unwrap();
    symlink(&target, discovery(&fixture)).unwrap();
    assert!(matches!(
        read_discovery(&fixture.paths, limits(), ctx.snapshot()),
        Err(DiscoveryError::Operation(_))
    ));
    assert!(publish_discovery(&auth, &socket, limits(), ctx.snapshot()).is_err());
    assert_eq!(
        fs::read(&target).unwrap(),
        br#"{"pid":1,"address":"127.0.0.1:1"}"#
    );
    fs::remove_file(discovery(&fixture)).unwrap();
    let record = publish_discovery(&auth, &socket, limits(), ctx.snapshot()).unwrap();
    fs::set_permissions(discovery(&fixture), fs::Permissions::from_mode(0o000)).unwrap();
    let result = read_discovery(&fixture.paths, limits(), ctx.snapshot());
    fs::set_permissions(discovery(&fixture), fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(result, Err(DiscoveryError::Operation(_))));
    assert!(remove_discovery(&auth, &record, limits(), ctx.snapshot()).unwrap());
    cleanup(&auth, ctx.snapshot());
}

#[test]
fn wildcard_listeners_publish_only_same_family_loopback_without_rotating_the_key() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let auth = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    let original_key = key(&auth, ctx.snapshot());
    for (bind, local) in [
        (
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        ),
        (
            IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
            IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        ),
    ] {
        let socket = tcp_listener::reserve(
            Options {
                address: bind,
                ports: PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &cancel,
        )
        .unwrap();
        let record = publish_discovery(&auth, &socket, limits(), ctx.snapshot()).unwrap();
        assert_eq!(socket.address().ip(), bind);
        assert_eq!(record.address().ip(), local);
        assert_eq!(record.address().port(), socket.address().port());
        assert!(std::net::TcpStream::connect(record.address()).is_ok());
        assert_eq!(
            read_discovery(&fixture.paths, limits(), ctx.snapshot())
                .unwrap()
                .as_ref(),
            Some(&record)
        );
        assert_eq!(key(&auth, ctx.snapshot()), original_key);
        assert!(remove_discovery(&auth, &record, limits(), ctx.snapshot()).unwrap());
    }
    cleanup(&auth, ctx.snapshot());
}
