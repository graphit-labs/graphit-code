//! Real Linux startup composition; public Broker journeys require its native fixture.
#![cfg(target_os = "linux")]
use graphit_app::foreground_source::{ForegroundFailure, ForegroundSource};
use graphit_auth::{
    profile::{
        AiService, BrokerService, LocalCredentials, OidcTokens, ProfileCredentials, ProfileData,
        ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
        Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource},
    file_lock::Cancellation,
    git_checkout::GitCheckout,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::{InstallationId, InstallationIdentityStore},
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectId, ProjectIdentityStore, ProjectMetadata},
    project_inventory::{CapturedProjectInventory, GlobalProjectInventory},
    project_paths::{InstallationPaths, ProjectPaths},
    version_context::VersionContext,
};
use graphit_daemon::{
    broker_inventory::{InventoryPublicationRemoteCode, OwnInventoryPublicationFailure},
    maintained_inventory::MaintainedProjectVersions,
    project_bootstrap::ProjectBootstrapInput,
    source_discovery,
};
use graphit_http::client::{
    BoundedClient, ClientPolicy, Limits, NetworkEffect, NetworkRequest, NetworkRequestContext,
};
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, RequestContext, ResourceSetting},
};
use reqwest::{
    Method,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    future::{Future, poll_fn},
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    task::Poll,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

fn config(paths: &InstallationPaths, values: &[(&str, Value)]) -> ConfigSnapshot {
    ConfigSnapshot::new(
        paths.brand(),
        [ConfigDocument::new(
            paths.brand(),
            ConfigSource::Command,
            values
                .iter()
                .map(|(key, value)| (ConfigKey::new(key).unwrap(), value.clone())),
        )
        .unwrap()],
    )
    .unwrap()
}
struct Fixture {
    directory: tempfile::TempDir,
    paths: InstallationPaths,
    project: ProjectPaths,
    config: ConfigSnapshot,
    policy: HubResourcePolicy,
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
        let root = directory.path().join("project");
        fs::create_dir(&root).unwrap();
        let project = ProjectPaths::at(&root, &paths).unwrap();
        let cancellation = Cancellation::default();
        ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &cancellation)
            .unwrap()
            .ensure(&ProjectMetadata::new("fixture-project").unwrap())
            .unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "initial",
            ],
            vec!["branch", "feature"],
        ] {
            let mut command = Command::new("git");
            for (key, _) in std::env::vars_os() {
                if key
                    .to_string_lossy()
                    .to_ascii_uppercase()
                    .starts_with("GIT_")
                {
                    command.env_remove(key);
                }
            }
            let result = command
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .args([
                    "-c",
                    "core.hooksPath=/dev/null",
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "tag.gpgsign=false",
                    "-c",
                    "gc.auto=0",
                ])
                .args(args)
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(result.status.success(), "temporary Git setup failed");
        }
        let config = config(&paths, &[("broker.allow_loopback_http", json!(true))]);
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        Self {
            directory,
            paths,
            project,
            config,
            policy,
        }
    }
    fn request<'a>(&self, cancel: &'a Cancellation) -> RequestContext<'a> {
        RequestContext::new(
            self.policy.content(),
            cancel,
            Instant::now() + Duration::from_secs(600),
        )
        .unwrap()
    }
    fn number(&self, setting: ResourceSetting) -> usize {
        usize::try_from(self.policy.limit(setting).value).unwrap()
    }
    fn native(&self) -> NativeDocumentFileLimits {
        NativeDocumentFileLimits::new(
            self.number(ResourceSetting::ContentInputBytes),
            graphit_core::native_path::MAX_PATH_BYTES,
            self.number(ResourceSetting::SnapshotDepth),
            self.number(ResourceSetting::ContentScalarBytes),
        )
        .unwrap()
    }
    fn scope(&self) -> ScopeLimits {
        ScopeLimits::new(
            4,
            graphit_core::native_path::MAX_PATH_BYTES,
            self.number(ResourceSetting::SnapshotDepth)
                .checked_mul(4)
                .unwrap(),
        )
        .unwrap()
    }
    fn prepare(
        &self,
        original: &RequestContext<'_>,
    ) -> graphit_core::installation_bootstrap::PrivateInstallation {
        installation_bootstrap::prepare(
            &self.paths,
            BootstrapLimits::new(
                graphit_core::native_path::MAX_PATH_BYTES,
                self.number(ResourceSetting::SnapshotDepth),
            )
            .unwrap(),
            original.snapshot(),
        )
        .into_installation()
        .unwrap()
    }
    fn id(&self, original: &RequestContext<'_>) -> InstallationId {
        self.prepare(original);
        InstallationIdentityStore::new(&self.paths)
            .unwrap()
            .ensure(
                original
                    .remaining()
                    .unwrap()
                    .expect("finite fixture deadline"),
                original.cancellation(),
            )
            .unwrap()
            .unit_id()
            .clone()
    }
    fn store(&self, original: &RequestContext<'_>) -> AuthStore {
        let id = self.id(original);
        let scope = CapturedPublicationScope::installation(
            &self.paths,
            id,
            [self.paths.root().to_owned()],
            self.scope(),
            original.snapshot(),
        )
        .unwrap();
        let bytes = self.number(ResourceSetting::ContentInputBytes);
        let scalar = self.number(ResourceSetting::ContentScalarBytes);
        let entries = self.number(ResourceSetting::ContentCollectionEntries);
        AuthStore::new(
            AuthDocumentStore::capture(
                scope,
                AuthDocumentLimits::new(
                    bytes,
                    graphit_core::native_path::MAX_PATH_BYTES,
                    self.number(ResourceSetting::SnapshotDepth),
                    scalar,
                )
                .unwrap(),
                original.snapshot(),
            )
            .unwrap(),
            ProfileLimits {
                bytes,
                nodes: self.number(ResourceSetting::ContentNodes),
                scalar_bytes: scalar,
                collection_entries: entries,
                providers: entries,
                profiles: entries,
            },
            original,
        )
        .unwrap()
    }
    fn profile(
        &self,
        kind: ProviderKind,
        credentials: ProfileCredentials,
        username: &str,
        original: &RequestContext<'_>,
    ) {
        let s3_source = if matches!(&kind, ProviderKind::Broker(_)) {
            S3Source::BrokerScope
        } else {
            S3Source::LocalCredentials
        };
        let store = self.store(original);
        let before = store.load(original).unwrap();
        let result = store
            .mutate(before, original, |doc, ctx| {
                doc.add_provider(
                    "inventory".into(),
                    ProviderConfig {
                        kind,
                        embedding: AiService::Disabled,
                        rerank: AiService::Disabled,
                        s3: S3Location {
                            bucket: None,
                            region: None,
                            endpoint: None,
                            prefix: None,
                            source: s3_source,
                        },
                    },
                    doc.revision(),
                    1,
                    ctx,
                )?;
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("inventory").unwrap().revision(),
                    profile_revision: None,
                };
                doc.replace_profile_data(
                    ProfileData {
                        name: "fixture".into(),
                        provider: "inventory".into(),
                        username: username.into(),
                        organization: None,
                        teams: vec![],
                        credentials,
                    },
                    expected,
                    1,
                    ctx,
                )?;
                doc.activate("fixture", doc.revision(), ctx)?;
                Ok(())
            })
            .unwrap();
        assert!(
            result.failure().is_none(),
            "temporary Auth publication failed"
        );
    }
    async fn start(
        &self,
        version: &str,
        original: &RequestContext<'_>,
    ) -> Result<ForegroundSource, ForegroundFailure> {
        ForegroundSource::start_and_publish(
            &self.paths,
            &[ProjectBootstrapInput {
                project: &self.project,
                explicit_version: version,
                pin: None,
                detached_base: "",
            }],
            0,
            &self.config,
            NativeBinding::Codex,
            Vec::new(),
            original,
        )
        .await
    }
    fn core(&self, original: &RequestContext<'_>) -> Vec<MaintainedProjectVersions> {
        GlobalProjectInventory::at(&self.paths, self.number(ResourceSetting::ContentInputBytes))
            .unwrap()
            .load(original.snapshot())
            .unwrap()
            .projects()
            .map(|p| MaintainedProjectVersions {
                project_id: p.identity().id(),
                versions: p.versions().map(str::to_owned).collect(),
            })
            .collect()
    }
    fn cleaned(&self) {
        assert!(!self.paths.root().join("daemon/mcp.json").exists());
        assert!(!self.paths.root().join("daemon/mcp.key").exists());
    }
    fn retain_history(&self, original: &RequestContext<'_>) -> ProjectPaths {
        let root = self.directory.path().join("historical");
        fs::create_dir(&root).unwrap();
        let project = ProjectPaths::at(&root, &self.paths).unwrap();
        ProjectIdentityStore::at(&project)
            .writer(
                original
                    .remaining()
                    .unwrap()
                    .expect("finite fixture deadline"),
                original.cancellation(),
            )
            .unwrap()
            .ensure(&ProjectMetadata::new("historical").unwrap())
            .unwrap();
        let checkout = GitCheckout::inspect(
            &root,
            original
                .remaining()
                .unwrap()
                .expect("finite fixture deadline"),
            original.cancellation(),
        )
        .unwrap();
        let selected =
            VersionContext::select(&project, checkout.as_checkout(), "old/β", None, "").unwrap();
        let capture = CapturedProjectInventory::capture(
            &project,
            &checkout,
            &selected,
            self.scope(),
            self.native(),
            original.snapshot(),
        )
        .unwrap();
        let store = GlobalProjectInventory::at(
            &self.paths,
            self.number(ResourceSetting::ContentInputBytes),
        )
        .unwrap();
        let mut writer = store
            .writer(self.prepare(original), original.snapshot())
            .unwrap();
        writer.register_checkout(&capture).unwrap();
        project
    }
}
fn local(key: Option<&str>) -> ProfileCredentials {
    ProfileCredentials::Local(LocalCredentials {
        anonymous: false,
        mcp_key: Some(Secret::new(SecretString::from("temporary-local-mcp"))),
        broker_key: key.map(|value| Secret::new(SecretString::from(value))),
        embedding_api_key: None,
        rerank_api_key: None,
        s3: None,
    })
}
fn broker_kind(endpoint: &str) -> ProviderKind {
    ProviderKind::Local {
        allow_aws_credential_chain: false,
        broker: Some(BrokerService {
            endpoint: endpoint.into(),
            mcp_resource: None,
            allow_anonymous: false,
        }),
    }
}
fn publication(failure: &ForegroundFailure) -> &OwnInventoryPublicationFailure {
    failure
        .error
        .get_ref()
        .unwrap()
        .downcast_ref()
        .expect("actual typed publication cause")
}

#[tokio::test]
async fn exact_absence_no_active_and_local_without_broker_use_real_local_startup() {
    for mode in 0..3 {
        let f = Fixture::new();
        let cancel = Cancellation::default();
        let original = f.request(&cancel);
        if mode == 1 {
            let store = f.store(&original);
            let snapshot = store.load(&original).unwrap();
            assert!(
                store
                    .publish(snapshot, &original)
                    .unwrap()
                    .failure()
                    .is_none()
            );
        } else if mode == 2 {
            f.profile(
                ProviderKind::Local {
                    allow_aws_credential_chain: false,
                    broker: None,
                },
                local(None),
                "fixture-user",
                &original,
            );
        }
        let owner = f.start("", &original).await.unwrap();
        assert!(owner.address().is_some());
        assert!(!f.core(&original).is_empty());
        owner.shutdown().await.unwrap();
        f.cleaned();
        f.start("", &original)
            .await
            .unwrap()
            .shutdown()
            .await
            .unwrap();
        f.cleaned();
    }
}

#[tokio::test]
async fn malformed_auth_and_untrusted_http_fail_before_runtime_owner() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    let original = f.request(&cancel);
    f.prepare(&original);
    fs::write(f.paths.root().join("auth.json"), b"malformed").unwrap();
    assert!(f.start("", &original).await.err().unwrap().owner.is_none());
    f.cleaned();
    fs::remove_file(f.paths.root().join("auth.json")).unwrap();
    f.profile(
        broker_kind("http://127.0.0.1:1"),
        local(Some("stored-key")),
        "fixture-user",
        &original,
    );
    let without_trust = config(&f.paths, &[]);
    let failure = ForegroundSource::start_and_publish(
        &f.paths,
        &[ProjectBootstrapInput {
            project: &f.project,
            explicit_version: "",
            pin: None,
            detached_base: "",
        }],
        0,
        &without_trust,
        NativeBinding::Codex,
        Vec::new(),
        &original,
    )
    .await
    .err()
    .unwrap();
    assert!(failure.owner.is_none());
    assert!(
        failure
            .error
            .get_ref()
            .unwrap()
            .is::<graphit_http::client::NetworkError>()
    );
    f.cleaned();
}

#[tokio::test]
async fn missing_key_and_unverified_native_retain_cleanup_owner() {
    for native in [false, true] {
        let f = Fixture::new();
        let cancel = Cancellation::default();
        let original = f.request(&cancel);
        let endpoint = "http://127.0.0.1:1";
        let (kind, credentials) = if native {
            (
                ProviderKind::Broker(BrokerService {
                    endpoint: endpoint.into(),
                    mcp_resource: None,
                    allow_anonymous: false,
                }),
                ProfileCredentials::Broker {
                    issuer: endpoint.into(),
                    subject: "stored-untrusted-assertion".into(),
                    tokens: OidcTokens {
                        access_token: Secret::new(SecretString::from("unverified-stored-access")),
                        refresh_token: None,
                        id_token: Secret::new(SecretString::from("unverified-stored-id")),
                        expires_at: 4102444800,
                    },
                    s3_disabled: true,
                },
            )
        } else {
            (
                ProviderKind::Local {
                    allow_aws_credential_chain: false,
                    broker: Some(BrokerService {
                        endpoint: endpoint.into(),
                        mcp_resource: None,
                        allow_anonymous: true,
                    }),
                },
                local(None),
            )
        };
        // Anonymous permission never supplies this non-anonymous profile's missing key.
        // Native now attempts verification; this unreachable endpoint cannot establish JWT authority.
        // NotSent describes inventory HTTP, not preceding Auth metadata work.
        f.profile(kind, credentials, "fixture-user", &original);
        let mut failure = f.start("", &original).await.err().unwrap();
        assert!(matches!(
            publication(&failure),
            OwnInventoryPublicationFailure::Admission { .. }
        ));
        assert_eq!(publication(&failure).effect(), NetworkEffect::NotSent);
        assert!(!f.core(&original).is_empty());
        failure.owner.take().unwrap().shutdown().await.unwrap();
        f.cleaned();
        let mut again = f.start("", &original).await.err().unwrap();
        again.owner.take().unwrap().shutdown().await.unwrap();
        f.cleaned();
    }
}

#[tokio::test]
async fn canceled_and_expired_originals_never_start_an_owner() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    let original = f.request(&cancel);
    cancel.cancel();
    assert!(f.start("", &original).await.err().unwrap().owner.is_none());
    f.cleaned();
    let cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_millis(5);
    let expired = RequestContext::new(f.policy.content(), &cancel, deadline).unwrap();
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        deadline + Duration::from_millis(1),
    ))
    .await;
    assert!(f.start("", &expired).await.err().unwrap().owner.is_none());
    f.cleaned();
}

fn network(endpoint: &str) -> BoundedClient {
    BoundedClient::new(
        ClientPolicy {
            origin: endpoint,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        Limits {
            request_body_bytes: 65536,
            response_body_bytes: 65536,
            url_bytes: 8192,
            header_bytes: 65536,
            header_count: 32,
            max_in_flight: 1,
            max_idle_per_host: 1,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 8,
            max_certificate_bytes: 65536,
        },
    )
    .unwrap()
}
async fn observed_if_published(ready: &Ready, id: &InstallationId) -> Option<Value> {
    observed_with_credential(&ready.endpoint, &ready.access_bearer, id).await
}
async fn observed_with_credential(
    endpoint: &str,
    bearer: &str,
    id: &InstallationId,
) -> Option<Value> {
    let cancel = Cancellation::default();
    let client = network(endpoint);
    let mut headers = HeaderMap::new();
    let mut authorization = HeaderValue::from_str(&format!("Bearer {bearer}")).unwrap();
    authorization.set_sensitive(true);
    headers.insert(AUTHORIZATION, authorization);
    let url = format!(
        "{}/v1/instances/{}/inventory",
        endpoint.trim_end_matches('/'),
        id.as_str()
    );
    let response = client
        .execute(
            NetworkRequest {
                method: Method::GET,
                url: &url,
                headers: &headers,
                body: &[],
            },
            NetworkRequestContext::new(Instant::now() + Duration::from_secs(30), &cancel),
        )
        .await
        .unwrap();
    let value: Value =
        serde_json::from_slice(response.body()).expect("public inventory response shape");
    if response.status().as_u16() == 404 {
        assert_eq!(value["error"]["code"], "not_found");
        return None;
    }
    assert_eq!(response.status().as_u16(), 200);
    Some(value)
}
async fn observed(ready: &Ready, id: &InstallationId) -> Value {
    observed_if_published(ready, id)
        .await
        .expect("own inventory must already be published")
}
fn whole(value: &Value, id: &InstallationId, projects: &[MaintainedProjectVersions]) {
    assert_eq!(value["instance_id"].as_str(), Some(id.as_str()));
    let expected: Vec<_> = projects
        .iter()
        .map(|p| {
            json!({"project_id": p.project_id,
        "versions": p.versions})
        })
        .collect();
    assert_eq!(value["projects"], json!(expected));
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ready {
    event: String,
    endpoint: String,
    instance_id: String,
    projects: Vec<ReadyProject>,
    audience: String,
    read_scopes: Vec<String>,
    publish_scopes: Vec<String>,
    access_bearer: String,
    service_bearer: String,
    subject: String,
    username: String,
    service_subject: String,
    service_username: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadyProject {
    project_id: ProjectId,
    versions: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ack {
    event: String,
    command: String,
    ok: bool,
}
/// Only the public process IPC; no stderr, Ready or credentials reach diagnostics.
struct BrokerProcess {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Result<String, ()>>,
    stdout: Option<JoinHandle<()>>,
    stderr: Option<JoinHandle<()>>,
    deadline: Instant,
}
impl BrokerProcess {
    fn start(
        id: &InstallationId,
        projects: &[MaintainedProjectVersions],
        deadline: Instant,
    ) -> Result<Self, &'static str> {
        let projects: Vec<Value> = projects
            .iter()
            .map(|p| json!({"project_id":p.project_id,"versions":p.versions}))
            .collect();
        Self::spawn(
            "GRAPHIT_BROKER_INVENTORY_FIXTURE_BIN",
            &json!({"command":"start","instance_id":id.as_str(),"projects":projects}),
            deadline,
        )
    }
    fn spawn(binary_input: &str, start: &Value, deadline: Instant) -> Result<Self, &'static str> {
        let binary = std::env::var_os(binary_input).ok_or("fixture executable input missing")?;
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| "fixture executable could not start")?;
        let input = child.stdin.take().ok_or("fixture stdin missing")?;
        let stdout = child.stdout.take().ok_or("fixture stdout missing")?;
        let stderr = child.stderr.take().ok_or("fixture stderr missing")?;
        let (send, output) = mpsc::channel();
        let stdout = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => {
                        let _ = send.send(Err(()));
                        break;
                    }
                    Ok(_) => {
                        if send.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let stderr = thread::spawn(move || {
            let mut source = stderr;
            let mut bytes = [0u8; 4096];
            while let Ok(n) = source.read(&mut bytes) {
                if n == 0 {
                    break;
                }
                bytes.fill(0)
            }
        });
        let mut process = Self {
            child,
            input: Some(input),
            output,
            stdout: Some(stdout),
            stderr: Some(stderr),
            deadline,
        };
        process.send(start)?;
        Ok(process)
    }
    fn send(&mut self, value: &Value) -> Result<(), &'static str> {
        let input = self.input.as_mut().ok_or("fixture input closed")?;
        serde_json::to_writer(&mut *input, value).map_err(|_| "fixture command write failed")?;
        input
            .write_all(b"\n")
            .and_then(|()| input.flush())
            .map_err(|_| "fixture command flush failed")
    }
    fn receive<T: serde::de::DeserializeOwned>(&self) -> Result<T, &'static str> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or("fixture Original deadline expired")?;
        let raw = self
            .output
            .recv_timeout(remaining)
            .map_err(|_| "fixture response outcome uncertain")?
            .map_err(|_| "fixture response stream closed")?;
        serde_json::from_str(&raw).map_err(|_| "fixture response malformed")
    }
    fn command(&mut self, command: &str) -> Result<(), &'static str> {
        self.send(&json!({"command":command}))?;
        let ack: Ack = self.receive()?;
        if ack.event != "ack" || ack.command != command || !ack.ok {
            return Err("fixture acknowledgement mismatch");
        };
        Ok(())
    }
    fn finish(mut self, shutdown: bool) -> Result<(), &'static str> {
        let ack = if shutdown {
            self.command("shutdown")
        } else {
            Ok(())
        };
        self.input.take();
        let status = self.child.wait().map_err(|_| "fixture reap failed")?;
        self.join();
        ack?;
        if !status.success() {
            return Err("fixture process failed");
        };
        Ok(())
    }
    fn join(&mut self) {
        if let Some(thread) = self.stdout.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.stderr.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for BrokerProcess {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.wait();
        self.join();
    }
}

fn validate_ready(ready: &Ready, id: &InstallationId) {
    assert_eq!(ready.event, "ready");
    assert_eq!(ready.instance_id, id.as_str());
    assert!(!ready.audience.is_empty());
    assert!(!ready.read_scopes.is_empty() && !ready.publish_scopes.is_empty());
    assert!(!ready.access_bearer.is_empty() && !ready.service_bearer.is_empty());
    assert!(!ready.subject.is_empty() && !ready.username.is_empty());
    assert!(!ready.service_subject.is_empty() && !ready.service_username.is_empty());
    assert!(ready.subject != ready.service_subject);
    for project in &ready.projects {
        assert!(project.project_id.to_string() != "00000000000000000000000000");
        assert!(!project.versions.is_empty());
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeReady {
    event: String,
    endpoint: String,
    issuer: String,
    inventory_endpoint: String,
    instance_id: String,
    api_audience: String,
    resource: String,
    client_id: String,
    subject: String,
    scope: String,
    redirect_uri_path: String,
    expires_at: u64,
    access_bearer: String,
    read_required_scopes: Vec<String>,
    publish_required_scopes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeStats {
    event: String,
    broker_metadata_requests: u64,
    oidc_metadata_requests: u64,
    jwks_requests: u64,
    token_requests: u64,
    userinfo_requests: u64,
    put_success_responses: u64,
    put_response_losses: u64,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires public prebuilt Native Broker fixture and native container runtime"]
async fn native_foreground_publishes_and_retains_owner_after_acl_denial() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    let original = f.request(&cancel);
    let id = f.id(&original);
    let mut broker = BrokerProcess::spawn(
        "GRAPHIT_BROKER_NATIVE_INVENTORY_FIXTURE_BIN",
        &json!({
            "command":"start",
            "api_audience":"graphit-code-native-inventory-fixture",
            "resource":"https://resource.example.test/runtime/inventory",
            "client_id":"graphit-code-runtime-fixture",
            "subject":"runtime-native-foreground-fixture",
            "scope":"openid presence:read presence:publish",
            "redirect_uri_path":"/callback",
            "access_ttl_seconds":600,
            "instance_id":id.as_str(),
            "projects":[],
            "seed_inventory":false,
            "grant_project_ids":["*"],
            "read_required_scopes":["presence:read"],
            "publish_required_scopes":["presence:publish"],
            "token_bytes":65536,
            "grants_json_bytes":65536,
            "inventory_json_bytes":65536,
            "body_bytes":65536,
            "request_timeout_ms":30000,
        }),
        original.deadline().expect("finite fixture deadline"),
    )
    .expect("public Native fixture setup failed");
    let ready: NativeReady = broker
        .receive()
        .expect("public Native fixture readiness failed");
    assert_eq!(ready.event, "ready");
    assert_eq!(ready.instance_id, id.as_str());
    assert_eq!(ready.endpoint, ready.issuer);
    assert_eq!(ready.endpoint, ready.inventory_endpoint);
    assert_eq!(ready.api_audience, "graphit-code-native-inventory-fixture");
    assert_eq!(
        ready.resource,
        "https://resource.example.test/runtime/inventory"
    );
    assert_eq!(ready.client_id, "graphit-code-runtime-fixture");
    assert_eq!(ready.subject, "runtime-native-foreground-fixture");
    assert_eq!(ready.scope, "openid presence:read presence:publish");
    assert_eq!(ready.redirect_uri_path, "/callback");
    assert_eq!(ready.read_required_scopes, ["presence:read"]);
    assert_eq!(ready.publish_required_scopes, ["presence:publish"]);
    f.profile(
        ProviderKind::Broker(BrokerService {
            endpoint: ready.endpoint.clone(),
            mcp_resource: Some(ready.resource.clone()),
            allow_anonymous: false,
        }),
        ProfileCredentials::Broker {
            issuer: ready.issuer.clone(),
            subject: ready.subject.clone(),
            tokens: OidcTokens {
                access_token: Secret::new(SecretString::from(ready.access_bearer.clone())),
                refresh_token: None,
                id_token: Secret::new(SecretString::from("fixture-stored-untrusted-id-data")),
                expires_at: i64::try_from(ready.expires_at)
                    .expect("fixture expiry exceeds profile bound"),
            },
            s3_disabled: true,
        },
        "fixture",
        &original,
    );
    let owner = f.start("", &original).await.unwrap();
    let created = observed_with_credential(&ready.endpoint, &ready.access_bearer, &id)
        .await
        .expect("Native foreground did not publish");
    whole(&created, &id, &f.core(&original));
    assert!(created["created_at"].as_i64().unwrap() > 0);
    owner.shutdown().await.unwrap();
    f.cleaned();
    broker.command("deny_publish").unwrap();
    let mut failure = f.start("feature", &original).await.err().unwrap();
    assert!(matches!(
        publication(&failure),
        OwnInventoryPublicationFailure::Remote {
            status: 403,
            code: InventoryPublicationRemoteCode::Forbidden,
            effect: NetworkEffect::MayHaveApplied,
        }
    ));
    let committed = f.core(&original);
    assert!(
        committed[0]
            .versions
            .iter()
            .any(|version| version == "feature")
    );
    failure
        .owner
        .take()
        .expect("Native failure lost its owner")
        .shutdown()
        .await
        .unwrap();
    f.cleaned();
    broker.command("allow_publish").unwrap();
    let owner = f.start("feature", &original).await.unwrap();
    let updated = observed_with_credential(&ready.endpoint, &ready.access_bearer, &id)
        .await
        .expect("Native foreground did not recover publication");
    whole(&updated, &id, &committed);
    assert_eq!(updated["created_at"], created["created_at"]);
    owner.shutdown().await.unwrap();
    f.cleaned();
    broker.send(&json!({"command":"stats"})).unwrap();
    let stats: NativeStats = broker.receive().unwrap();
    assert_eq!(stats.event, "stats");
    assert!(stats.broker_metadata_requests > 0);
    assert!(stats.oidc_metadata_requests > 0);
    assert!(stats.jwks_requests > 0);
    assert_eq!(stats.token_requests, 0);
    assert_eq!(stats.userinfo_requests, 0);
    assert_eq!(stats.put_success_responses, 2);
    assert_eq!(stats.put_response_losses, 0);
    broker
        .finish(true)
        .expect("public Native fixture cleanup failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires public prebuilt Broker fixture executable and native container runtime"]
async fn real_broker_startup_restart_history_and_failures_preserve_local_commit_and_owner() {
    let mut f = Fixture::new();
    let cancel = Cancellation::default();
    let original = f.request(&cancel);
    let id = f.id(&original);
    let mut broker = BrokerProcess::start(
        &id,
        &[],
        original.deadline().expect("finite fixture deadline"),
    )
    .expect("public fixture setup failed");
    let ready: Ready = broker.receive().expect("public fixture readiness failed");
    validate_ready(&ready, &id);
    f.profile(
        broker_kind(&ready.endpoint),
        local(Some(&ready.access_bearer)),
        &ready.username,
        &original,
    );
    broker.command("delete_inventory").unwrap();
    let owner = f.start("", &original).await.unwrap();
    let created = observed(&ready, &id).await;
    whole(&created, &id, &f.core(&original));
    assert!(created["created_at"].as_i64().unwrap() > 0);
    let duplicate = f.start("", &original).await.err().unwrap();
    assert!(duplicate.owner.is_none());
    owner.shutdown().await.unwrap();
    f.cleaned();
    let response = created.clone();
    let before_bound = f.core(&original);
    let request_bound = serde_json::to_vec(&json!({"projects": response["projects"]}))
        .unwrap()
        .len();
    assert!(
        serde_json::to_vec(&json!({
            "instance_id": id.as_str(), "change": "unchanged",
            "created_at": response["created_at"], "updated_at": response["updated_at"]
        }))
        .unwrap()
        .len()
            > request_bound
    );
    f.config = config(
        &f.paths,
        &[
            ("broker.allow_loopback_http", json!(true)),
            (
                ResourceSetting::TransportBytes.spec().key,
                json!(request_bound),
            ),
            (
                ResourceSetting::TransportScalarBytes.spec().key,
                json!(request_bound),
            ),
        ],
    );
    f.policy = HubResourcePolicy::resolve(&f.config).unwrap();
    let mut failure = f.start("", &original).await.err().unwrap();
    assert!(matches!(
        publication(&failure),
        OwnInventoryPublicationFailure::Network {
            source: graphit_http::client::NetworkError::Limit,
            effect: NetworkEffect::MayHaveApplied,
        }
    ));
    assert_eq!(f.core(&original), before_bound);
    whole(&observed(&ready, &id).await, &id, &before_bound);
    failure.owner.take().unwrap().shutdown().await.unwrap();
    f.cleaned();

    f.config = config(&f.paths, &[("broker.allow_loopback_http", json!(true))]);
    f.policy = HubResourcePolicy::resolve(&f.config).unwrap();
    let owner = f.start("", &original).await.unwrap();
    assert_eq!(observed(&ready, &id).await, created);
    owner.shutdown().await.unwrap();
    f.cleaned();
    let owner = f.start("feature", &original).await.unwrap();
    let updated = observed(&ready, &id).await;
    whole(&updated, &id, &f.core(&original));
    assert_eq!(updated["created_at"], created["created_at"]);
    assert!(updated["updated_at"].as_i64().unwrap() >= created["updated_at"].as_i64().unwrap());
    owner.shutdown().await.unwrap();
    f.cleaned();
    let historical = f.retain_history(&original);
    fs::remove_dir_all(historical.root()).unwrap();
    // SDK inputs deliberately contain only the valid project; this is not CLI selector proof.
    let owner = f.start("feature", &original).await.unwrap();
    whole(&observed(&ready, &id).await, &id, &f.core(&original));
    assert!(!historical.root().exists());
    owner.shutdown().await.unwrap();
    f.cleaned();

    broker.command("revoke_access").unwrap();
    let before_failure = f.core(&original);
    let mut failure = f.start("feature", &original).await.err().unwrap();
    assert!(matches!(
        publication(&failure),
        OwnInventoryPublicationFailure::Remote {
            status: 401,
            code: InventoryPublicationRemoteCode::Unauthenticated,
            ..
        }
    ));
    assert_eq!(
        publication(&failure).effect(),
        NetworkEffect::MayHaveApplied
    );
    assert_eq!(f.core(&original), before_failure);
    failure.owner.take().unwrap().shutdown().await.unwrap();
    f.cleaned();
    broker.command("restore_access").unwrap();
    f.start("feature", &original)
        .await
        .unwrap()
        .shutdown()
        .await
        .unwrap();
    f.cleaned();

    {
        let during = Cancellation::default();
        let during_original = f.request(&during);
        let future = f.start("feature", &during_original);
        tokio::pin!(future);
        let completed = poll_fn(|cx| match future.as_mut().poll(cx) {
            Poll::Ready(result) => Poll::Ready(Some(result)),
            Poll::Pending => {
                during.cancel();
                Poll::Ready(None)
            }
        })
        .await;
        assert!(
            completed.is_none(),
            "real native request must reach await boundary"
        );
        let mut failure = future.await.err().unwrap();
        assert_eq!(
            publication(&failure).effect(),
            NetworkEffect::MayHaveApplied
        );
        assert_eq!(f.core(&original), before_failure);
        failure.owner.take().unwrap().shutdown().await.unwrap();
        f.cleaned();
    }
    {
        let expiry_cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(2);
        let expiry = RequestContext::new(f.policy.content(), &expiry_cancel, deadline).unwrap();
        let future = f.start("feature", &expiry);
        tokio::pin!(future);
        let completed = poll_fn(|cx| match future.as_mut().poll(cx) {
            Poll::Ready(result) => Poll::Ready(Some(result)),
            Poll::Pending => Poll::Ready(None),
        })
        .await;
        assert!(
            completed.is_none(),
            "real native request must reach await boundary"
        );
        tokio::time::sleep_until(tokio::time::Instant::from_std(
            deadline + Duration::from_millis(1),
        ))
        .await;
        let mut failure = future.await.err().unwrap();
        assert_eq!(
            publication(&failure).effect(),
            NetworkEffect::MayHaveApplied
        );
        assert_eq!(f.core(&original), before_failure);
        failure.owner.take().unwrap().shutdown().await.unwrap();
        f.cleaned();
    }
    f.config = config(
        &f.paths,
        &[
            ("broker.allow_loopback_http", json!(true)),
            (ResourceSetting::TransportBytes.spec().key, json!(64)),
            (ResourceSetting::TransportScalarBytes.spec().key, json!(64)),
        ],
    );
    f.policy = HubResourcePolicy::resolve(&f.config).unwrap();
    let mut failure = f.start("feature", &original).await.err().unwrap();
    assert!(matches!(
        publication(&failure),
        OwnInventoryPublicationFailure::Encoding { .. }
    ));
    assert_eq!(publication(&failure).effect(), NetworkEffect::NotSent);
    assert_eq!(f.core(&original), before_failure);
    failure.owner.take().unwrap().shutdown().await.unwrap();
    f.cleaned();
    broker.finish(true).unwrap();
}

struct CliProcess {
    child: Child,
    stdout: Option<JoinHandle<usize>>,
    stderr: Option<JoinHandle<usize>>,
}
fn drain(mut input: impl Read + Send + 'static) -> JoinHandle<usize> {
    thread::spawn(move || {
        let mut bytes = [0u8; 4096];
        let mut count = 0;
        while let Ok(n) = input.read(&mut bytes) {
            if n == 0 {
                break;
            }
            count += n;
            bytes.fill(0);
        }
        count
    })
}
impl CliProcess {
    fn start(f: &Fixture) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_graphit"))
            .current_dir(f.project.root())
            .arg("--global-dir")
            .arg(f.paths.root())
            .args([
                "-c",
                &format!("agent={}", NativeBinding::Codex.as_str()),
                "-c",
                "broker.allow_loopback_http=true",
                "daemon",
                "foreground",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = Some(drain(child.stdout.take().unwrap()));
        let stderr = Some(drain(child.stderr.take().unwrap()));
        Self {
            child,
            stdout,
            stderr,
        }
    }
    async fn finish(mut self, signal: &str) {
        let result = Command::new("kill")
            .args(["-s", signal, &self.child.id().to_string()])
            .output()
            .unwrap();
        assert!(result.status.success(), "native signal delivery failed");
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "foreground did not exit after signal"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(status.code(), Some(130));
        assert_eq!(self.stdout.take().unwrap().join().unwrap(), 0);
        self.stderr.take().unwrap().join().unwrap();
    }
    fn join(&mut self) {
        if let Some(thread) = self.stdout.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.stderr.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for CliProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.join();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires public prebuilt Broker fixture executable and native container runtime"]
async fn compiled_cli_publishes_whole_inventory_and_cleans_up_on_sigint_and_sigterm() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    let original = f.request(&cancel);
    let id = f.id(&original);
    let mut broker = BrokerProcess::start(
        &id,
        &[],
        original.deadline().expect("finite fixture deadline"),
    )
    .expect("public fixture setup failed");
    let ready: Ready = broker.receive().expect("public fixture readiness failed");
    validate_ready(&ready, &id);
    f.profile(
        broker_kind(&ready.endpoint),
        local(Some(&ready.access_bearer)),
        &ready.username,
        &original,
    );
    for signal in ["INT", "TERM"] {
        broker.command("delete_inventory").unwrap();
        assert!(observed_if_published(&ready, &id).await.is_none());
        let mut cli = CliProcess::start(&f);
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            assert!(
                cli.child.try_wait().unwrap().is_none(),
                "compiled foreground exited during startup"
            );
            if source_discovery::read_discovery(&f.paths, f.native(), original.snapshot())
                .unwrap()
                .is_some()
            {
                let Some(value) = observed_if_published(&ready, &id).await else {
                    assert!(
                        Instant::now() < deadline,
                        "compiled foreground inventory was not published"
                    );
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    continue;
                };
                let expected = f.core(&original);
                whole(&value, &id, &expected);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "compiled foreground discovery did not appear"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        cli.finish(signal).await;
        f.cleaned();
    }
    // Existing CLI selection still includes missing historical roots and must fail visibly.
    let missing = f.retain_history(&original);
    fs::remove_dir_all(missing.root()).unwrap();
    let mut cli = CliProcess::start(&f);
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = cli.child.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "missing-checkout startup did not fail"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(status.code(), Some(1));
    f.cleaned();
    cli.join();
    drop(cli);
    broker.finish(true).unwrap();
}
