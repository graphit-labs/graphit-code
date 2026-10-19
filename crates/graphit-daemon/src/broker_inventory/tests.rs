use super::*;
use serde_json::{Value, json};

const INSTANCE: &str = "fixture-installation";
const PROJECT: &str = "01M1YEAE1WYMWG9KJKV1XQC4T4";
fn valid_wire() -> Value {
    json!({"instance_id":INSTANCE,"projects":[{"project_id":PROJECT,"versions":["main","feature/topic","versão"]}],"created_at":1234567,"updated_at":0})
}
fn decode(value: &Value) -> Result<OwnInventory, ()> {
    decode_inventory(
        &serde_json::to_vec(value).unwrap(),
        &InstallationId::new(INSTANCE).unwrap(),
    )
}
#[test]
fn broker_inventory_decodes_exact_labels_microseconds_and_empty() {
    let wire = valid_wire();
    let result = decode(&wire).unwrap();
    assert_eq!(result.instance_id.as_str(), INSTANCE);
    assert_eq!(result.projects[0].project_id.to_string(), PROJECT);
    assert_eq!(
        result.projects[0].versions,
        ["main", "feature/topic", "versão"]
    );
    assert_eq!((result.created_at, result.updated_at), (1234567, 0));
    let mut empty = wire;
    empty["projects"] = json!([]);
    assert!(decode(&empty).unwrap().projects.is_empty());
}
#[test]
fn broker_inventory_rejects_invalid_wire_without_normalizing() {
    let mut cases = Vec::new();
    for id in [
        " fixture-installation",
        "fixture-installation ",
        "other",
        "",
        "bad/id",
    ] {
        let mut v = valid_wire();
        v["instance_id"] = json!(id);
        cases.push(v);
    }
    for id in [
        "00000000000000000000000000",
        "01m1yeae1wymwg9kjkv1xqc4t4",
        "bad",
    ] {
        let mut v = valid_wire();
        v["projects"][0]["project_id"] = json!(id);
        cases.push(v);
    }
    for labels in [
        json!([]),
        json!([""]),
        json!([" main"]),
        json!(["main "]),
        json!(["line\nfeed"]),
        json!(["main", "main"]),
        json!([null]),
    ] {
        let mut v = valid_wire();
        v["projects"][0]["versions"] = labels;
        cases.push(v);
    }
    let mut duplicate = valid_wire();
    duplicate["projects"] = json!([
        duplicate["projects"][0].clone(),
        duplicate["projects"][0].clone()
    ]);
    cases.push(duplicate);
    for field in ["created_at", "updated_at"] {
        for timestamp in [
            json!(-1),
            json!(1.5),
            json!("123"),
            json!(null),
            json!(u64::MAX),
        ] {
            let mut v = valid_wire();
            v[field] = timestamp;
            cases.push(v);
        }
    }
    for field in ["instance_id", "projects", "created_at", "updated_at"] {
        let mut v = valid_wire();
        v.as_object_mut().unwrap().remove(field);
        cases.push(v);
    }
    let mut v = valid_wire();
    v["owner"] = json!("private");
    cases.push(v);
    let mut v = valid_wire();
    v["projects"][0]["extra"] = json!(true);
    cases.push(v);
    for (index, v) in cases.iter().enumerate() {
        assert!(decode(v).is_err(), "case {index}");
    }
    let raw = serde_json::to_string(&valid_wire()).unwrap();
    let duplicate = raw.replacen("{", "{\"instance_id\":\"fixture-installation\",", 1);
    for bytes in [duplicate, format!("{raw} {{}}"), format!("{raw} trailing")] {
        assert!(
            decode_inventory(bytes.as_bytes(), &InstallationId::new(INSTANCE).unwrap()).is_err()
        );
    }
}
#[test]
fn broker_inventory_remote_codes_are_closed_and_status_bound() {
    let pairs = [
        (401, "unauthenticated"),
        (403, "forbidden"),
        (400, "invalid_inventory"),
        (409, "owner_conflict"),
        (404, "not_found"),
        (503, "authentication_unavailable"),
        (503, "authorization_unavailable"),
        (503, "invalid_policy"),
        (503, "inventory_unavailable"),
        (503, "closed"),
        (503, "outcome_uncertain"),
        (504, "deadline"),
        (405, "method_not_allowed"),
    ];
    for (status, code) in pairs {
        let bytes = serde_json::to_vec(&json!({"error":{"code":code}})).unwrap();
        assert!(decode_remote(&bytes, status).is_ok());
        assert!(decode_remote(&bytes, 200).is_err());
    }
    for wire in [
        r#"{"error":{"code":"unknown"}}"#,
        r#"{"error":{"code":"not_found","detail":"private"}}"#,
        r#"{"error":{"code":"not_found"},"extra":true}"#,
        r#"{"error":{"code":"not_found","code":"not_found"}}"#,
        r#"{"error":{"code":"not_found"}} {}"#,
    ] {
        assert!(decode_remote(wire.as_bytes(), 404).is_err());
    }
}
#[test]
fn broker_inventory_url_preserves_prefix_and_escapes() {
    let id = InstallationId::new(INSTANCE).unwrap();
    for (endpoint, expected) in [
        (
            "https://example.test/api",
            "https://example.test/api/v1/instances/fixture-installation/inventory",
        ),
        (
            "https://example.test/api/",
            "https://example.test/api/v1/instances/fixture-installation/inventory",
        ),
        (
            "https://example.test/a%20b/@path",
            "https://example.test/a%20b/@path/v1/instances/fixture-installation/inventory",
        ),
    ] {
        assert_eq!(inventory_url(endpoint, &id).unwrap().as_str(), expected);
    }
}

use crate::auth_startup::StartupPolicy;
use graphit_auth::{
    AdminSessionPolicy, CredentialSource, ExternalEffect, IssuerAuthorityPolicy, IssuerLimits,
    Permission, RemoteServiceBindingInput, ServiceAdmissionLimits, ServiceAuthorityInput,
    ServiceOrigin, ServiceRuleInput, SourceAuthorityPolicy, TtlPolicy,
    broker::{JsonLimits, VerifierLimits},
    profile::{
        AiService, BrokerService, LocalCredentials, OidcTokens, ProfileCredentials, ProfileData,
        ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
        Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    auth_document::{AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect},
    brand::Brand,
    file_lock::Cancellation,
    git_checkout::GitCheckout,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationIdentityStore,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_inventory::{CapturedProjectInventory, GlobalProjectInventory},
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::RuntimeKeyLimits,
    version_context::{Checkout, VersionContext},
};
use graphit_http::client::{ClientPolicy, Limits};
use graphit_hub::native::ContentLimits;
use secrecy::SecretString;
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

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
fn startup(endpoint: &str) -> StartupPolicy {
    startup_with_purposes(endpoint, &[ServicePurpose::InventoryRead])
}
fn startup_with_purposes(endpoint: &str, purposes: &[ServicePurpose]) -> StartupPolicy {
    startup_with_credential(endpoint, purposes, CredentialSource::LocalBrokerKey)
}
fn startup_with_credential(
    endpoint: &str,
    purposes: &[ServicePurpose],
    credential: CredentialSource,
) -> StartupPolicy {
    let permissions = [Permission::External(ExternalEffect::RemoteRequest)];
    let source = || SourceAuthorityPolicy {
        ordinary: &permissions,
        administrative: &[
            Permission::ManageAccessGrants,
            Permission::External(ExternalEffect::RemoteRequest),
        ],
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    let origins: &[ServiceOrigin] = if matches!(credential, CredentialSource::NativeAccess) {
        &[ServiceOrigin::Runtime]
    } else {
        &[ServiceOrigin::Runtime, ServiceOrigin::Local]
    };
    let rules: Vec<_> = origins
        .iter()
        .copied()
        .flat_map(|origin| {
            purposes
                .iter()
                .map(move |purpose| ServiceRuleInput::Remote {
                    origin,
                    binding: RemoteServiceBindingInput::BrokerInventory {
                        provider: "inventory",
                        purpose: *purpose,
                        endpoint,
                    },
                    credential,
                    incoming: None,
                })
        })
        .collect();
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
                rules: &rules,
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
fn network(endpoint: &str, response_bytes: usize) -> BoundedClient {
    network_with_request_bound(endpoint, response_bytes, 65536)
}
fn network_with_request_bound(
    endpoint: &str,
    response_bytes: usize,
    request_bytes: usize,
) -> BoundedClient {
    BoundedClient::new(
        ClientPolicy {
            origin: endpoint,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        Limits {
            request_body_bytes: request_bytes,
            response_body_bytes: response_bytes,
            url_bytes: 8192,
            header_bytes: 8192,
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
struct TemporaryInstallation {
    directory: tempfile::TempDir,
    paths: InstallationPaths,
}
impl TemporaryInstallation {
    fn new(original: &RequestContext<'_>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let owner = directory.path().join("installation");
        let paths =
            InstallationPaths::resolve(Brand::default(), directory.path(), None, Some(&owner))
                .unwrap();
        installation_bootstrap::prepare(
            &paths,
            BootstrapLimits::new(65536, 256).unwrap(),
            original.snapshot(),
        )
        .into_installation()
        .unwrap();
        Self { directory, paths }
    }
    fn projects(
        &self,
        original: &RequestContext<'_>,
    ) -> (Vec<ProjectPaths>, Vec<MaintainedProjectVersions>) {
        let mut paths = Vec::new();
        let mut projects = Vec::new();
        for (name, label) in [("first", ""), ("second", "feature/topic")] {
            let root = self.directory.path().join(name);
            std::fs::create_dir(&root).unwrap();
            let project = ProjectPaths::at(root, &self.paths).unwrap();
            let identity = ProjectIdentityStore::at(&project)
                .writer(Duration::from_secs(5), original.cancellation())
                .unwrap()
                .ensure(&ProjectMetadata::new(name).unwrap())
                .unwrap();
            let version =
                VersionContext::select(&project, Checkout::NonGit, label, None, "").unwrap();
            projects.push(MaintainedProjectVersions {
                project_id: identity.id(),
                versions: vec![version.version().to_owned()],
            });
            paths.push(project);
        }
        projects.sort_by_key(|p| p.project_id.to_string());
        (paths, projects)
    }
}
fn profile(
    auth: &DaemonAuth,
    paths: &InstallationPaths,
    endpoint: &str,
    key: &str,
    username: &str,
    name: &str,
    original: &RequestContext<'_>,
) {
    install_profile(
        auth,
        paths,
        ProviderKind::Local {
            allow_aws_credential_chain: false,
            broker: Some(BrokerService {
                endpoint: endpoint.into(),
                mcp_resource: None,
                allow_anonymous: false,
            }),
        },
        ProfileData {
            name: name.into(),
            provider: "inventory".into(),
            username: username.into(),
            organization: None,
            teams: vec![],
            credentials: ProfileCredentials::Local(LocalCredentials {
                anonymous: false,
                mcp_key: Some(Secret::new(SecretString::from(
                    "runtime-inventory-local-key",
                ))),
                broker_key: Some(Secret::new(SecretString::from(key))),
                embedding_api_key: None,
                rerank_api_key: None,
                s3: None,
            }),
        },
        original,
    );
}
fn install_profile(
    auth: &DaemonAuth,
    paths: &InstallationPaths,
    kind: ProviderKind,
    data: ProfileData,
    original: &RequestContext<'_>,
) {
    let StoreOwner::Installation { installation_id } = auth.installation().live.owner() else {
        panic!("installation owner required")
    };
    let scope = CapturedPublicationScope::installation(
        paths,
        installation_id.clone(),
        auth.installation()
            .live
            .allowed_roots()
            .map(ToOwned::to_owned),
        scopes(),
        original.snapshot(),
    )
    .unwrap();
    let store = AuthStore::new(
        AuthDocumentStore::capture(scope, documents(), original.snapshot()).unwrap(),
        profiles(),
        original,
    )
    .unwrap();
    let before = store.load(original).unwrap();
    let result = store
        .mutate(before, original, |doc, ctx| {
            let s3_source = match &kind {
                ProviderKind::Broker(_) => S3Source::BrokerScope,
                ProviderKind::Local { .. } => S3Source::LocalCredentials,
            };
            if doc.provider("inventory").is_none() {
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
            }
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("inventory").unwrap().revision(),
                profile_revision: None,
            };
            let profile_name = data.name.clone();
            doc.replace_profile_data(data, expected, 1, ctx)?;
            doc.activate(&profile_name, doc.revision(), ctx)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert!(result.failure().is_none());
}
fn inventory_verifier_cache() -> BrokerVerifierCache {
    BrokerVerifierCache::new(
        Limits {
            request_body_bytes: 65536,
            response_body_bytes: 65536,
            url_bytes: 8192,
            header_bytes: 8192,
            header_count: 32,
            max_in_flight: 1,
            max_idle_per_host: 1,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 8,
            max_certificate_bytes: 65536,
        },
        VerifierLimits {
            token_bytes: 4096,
            json: JsonLimits {
                bytes: 65536,
                nodes: 4096,
                depth: 32,
                scalar_bytes: 4096,
                collection_entries: 64,
            },
            max_keys: 64,
            metadata_ttl: Duration::from_secs(300),
            key_ttl: Duration::from_secs(300),
            unknown_key_cooldown: Duration::from_secs(1),
        },
        true,
        None,
        &[],
    )
}

#[tokio::test]
async fn native_inventory_rejects_other_original_before_verification_and_never_uses_local() {
    let cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let original = request_until(&cancel, deadline);
    let other = request_until(&cancel, deadline);
    let temp = TemporaryInstallation::new(&original);
    let (paths, _) = temp.projects(&original);
    let endpoint = "http://127.0.0.1:1";
    let auth = DaemonAuth::start(
        &temp.paths,
        &paths,
        startup_with_credential(
            endpoint,
            &[
                ServicePurpose::InventoryRead,
                ServicePurpose::InventoryPublish,
            ],
            CredentialSource::NativeAccess,
        ),
        &original,
    )
    .unwrap();
    install_profile(
        &auth,
        &temp.paths,
        ProviderKind::Broker(BrokerService {
            endpoint: endpoint.into(),
            mcp_resource: None,
            allow_anonymous: false,
        }),
        ProfileData {
            name: "native".into(),
            provider: "inventory".into(),
            username: "fixture".into(),
            organization: None,
            teams: vec![],
            credentials: ProfileCredentials::Broker {
                issuer: endpoint.into(),
                subject: "fixture-native".into(),
                s3_disabled: true,
                tokens: OidcTokens {
                    access_token: Secret::new(SecretString::from("not-a-jwt")),
                    refresh_token: None,
                    id_token: Secret::new(SecretString::from("not-an-id-token")),
                    // Renewable OAuth profile expiry is stored in UTC microseconds.
                    // Keep it valid so the malformed JWT reaches Broker verification.
                    expires_at: 4102444800000000,
                },
            },
        },
        &original,
    );
    let key = runtime_key(&auth, &original);
    let caller = auth.issuer().authenticate_runtime(&key, &original).unwrap();
    let cache = inventory_verifier_cache();
    // Cache construction/reuse does not verify this malformed stored token.
    let mut selection = auth
        .issuer()
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    let first = cache.get(&mut selection).unwrap();
    let second = cache.get(&mut selection).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    let client = network(endpoint, 65536);
    assert!(matches!(
        read_own_inventory(
            &client,
            &auth,
            &caller,
            InventoryCredentialMode::BrokerNative(&cache),
            &other,
        )
        .await,
        Err(OwnInventoryReadFailure::Admission {
            source: InventoryAdmissionFailure::DifferentOriginal,
            effect: NetworkEffect::NotSent,
        })
    ));
    assert!(matches!(
        publish_maintained_inventory(
            &client,
            &auth,
            &caller,
            InventoryCredentialMode::BrokerNative(&cache),
            &other,
            65536,
        )
        .await,
        Err(OwnInventoryPublicationFailure::Admission {
            source: InventoryAdmissionFailure::DifferentOriginal,
            effect: NetworkEffect::NotSent,
        })
    ));
    assert!(matches!(
        read_own_inventory(
            &client,
            &auth,
            &caller,
            InventoryCredentialMode::LocalBrokerKey,
            &original,
        )
        .await,
        Err(OwnInventoryReadFailure::Admission {
            source: InventoryAdmissionFailure::Auth(ServiceUseError::WrongMode),
            effect: NetworkEffect::NotSent,
        })
    ));
    // Native failure keeps the actual Broker cause; it does not retry Local.
    assert!(matches!(
        read_own_inventory(
            &client,
            &auth,
            &caller,
            InventoryCredentialMode::BrokerNative(&cache),
            &original,
        )
        .await,
        Err(OwnInventoryReadFailure::Admission {
            source: InventoryAdmissionFailure::Auth(ServiceUseError::Broker(_)),
            effect: NetworkEffect::NotSent,
        })
    ));
    assert!(matches!(
        publish_maintained_inventory(
            &client,
            &auth,
            &caller,
            InventoryCredentialMode::BrokerNative(&cache),
            &original,
            65536,
        )
        .await,
        Err(OwnInventoryPublicationFailure::Admission {
            source: InventoryAdmissionFailure::Auth(ServiceUseError::Broker(_)),
            effect: NetworkEffect::NotSent,
        })
    ));
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
        self.command_value(command, &json!({"command":command}))
    }
    fn command_value(&mut self, command: &str, value: &Value) -> Result<(), &'static str> {
        self.send(value)?;
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
fn runtime_key(auth: &DaemonAuth, original: &RequestContext<'_>) -> String {
    let checked = auth.runtime().check(original.snapshot()).unwrap();
    String::from_utf8(checked.secret(original.snapshot()).unwrap().to_vec()).unwrap()
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

#[tokio::test]
#[ignore = "requires public prebuilt Native Broker fixture and native container runtime"]
async fn broker_inventory_native_access_process_journeys() {
    let cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(600);
    let original = request_until(&cancel, deadline);
    let temp = TemporaryInstallation::new(&original);
    let (paths, projects) = temp.projects(&original);
    maintain_project(&temp, &paths[0], "main", &[], &original);
    maintain_project(&temp, &paths[1], "feature/topic", &[], &original);
    let facts = core_projects(&temp, &original);
    assert_eq!(facts, projects);
    let id = InstallationIdentityStore::new(&temp.paths)
        .unwrap()
        .ensure(Duration::from_secs(5), original.cancellation())
        .unwrap()
        .unit_id()
        .clone();
    let wire_projects: Vec<Value> = facts
        .iter()
        .map(|p| json!({"project_id":p.project_id,"versions":p.versions}))
        .collect();
    let start = json!({
        "command":"start",
        "api_audience":"graphit-code-native-inventory-fixture",
        "resource":"https://resource.example.test/runtime/inventory",
        "client_id":"graphit-code-runtime-fixture",
        "subject":"runtime-native-fixture",
        "scope":"openid presence:read presence:publish",
        "redirect_uri_path":"/callback",
        "access_ttl_seconds":600,
        "instance_id":id.as_str(),
        "projects":wire_projects,
        "seed_inventory":false,
        "grant_project_ids":["*"],
        "read_required_scopes":["presence:read"],
        "publish_required_scopes":["presence:publish"],
        "token_bytes":65536,
        "grants_json_bytes":65536,
        "inventory_json_bytes":65536,
        "body_bytes":65536,
        "request_timeout_ms":30000,
    });
    let mut broker = BrokerProcess::spawn(
        "GRAPHIT_BROKER_NATIVE_INVENTORY_FIXTURE_BIN",
        &start,
        deadline,
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
    assert_eq!(ready.subject, "runtime-native-fixture");
    assert_eq!(ready.scope, "openid presence:read presence:publish");
    assert_eq!(ready.redirect_uri_path, "/callback");
    assert_eq!(ready.read_required_scopes, ["presence:read"]);
    assert_eq!(ready.publish_required_scopes, ["presence:publish"]);
    let expiry = i64::try_from(ready.expires_at).expect("fixture expiry exceeds profile bound");
    let auth = DaemonAuth::start(
        &temp.paths,
        &paths,
        startup_with_credential(
            &ready.endpoint,
            &[
                ServicePurpose::InventoryRead,
                ServicePurpose::InventoryPublish,
            ],
            CredentialSource::NativeAccess,
        ),
        &original,
    )
    .unwrap();
    install_profile(
        &auth,
        &temp.paths,
        ProviderKind::Broker(BrokerService {
            endpoint: ready.endpoint.clone(),
            mcp_resource: Some(ready.resource.clone()),
            allow_anonymous: false,
        }),
        ProfileData {
            name: "native".into(),
            provider: "inventory".into(),
            username: "fixture".into(),
            organization: None,
            teams: vec![],
            credentials: ProfileCredentials::Broker {
                issuer: ready.issuer.clone(),
                subject: ready.subject.clone(),
                tokens: OidcTokens {
                    access_token: Secret::new(SecretString::from(ready.access_bearer)),
                    refresh_token: None,
                    // Historical untrusted profile data; this journey verifies access only.
                    id_token: Secret::new(SecretString::from("fixture-stored-untrusted-id-data")),
                    expires_at: expiry,
                },
                s3_disabled: true,
            },
        },
        &original,
    );
    let key = runtime_key(&auth, &original);
    let caller = auth.issuer().authenticate_runtime(&key, &original).unwrap();
    let cache = inventory_verifier_cache();
    let mode = InventoryCredentialMode::BrokerNative(&cache);
    let client = network(&ready.endpoint, 65536);
    remote_failure(
        read_own_inventory(&client, &auth, &caller, mode, &original).await,
        404,
        InventoryRemoteCode::NotFound,
    );
    let created = publish_maintained_inventory(&client, &auth, &caller, mode, &original, 65536)
        .await
        .unwrap();
    assert_eq!(created.change, OwnInventoryChange::Created);
    assert_eq!(
        read_own_inventory(&client, &auth, &caller, mode, &original)
            .await
            .unwrap()
            .projects,
        facts,
    );
    assert_eq!(
        publish_maintained_inventory(&client, &auth, &caller, mode, &original, 65536)
            .await
            .unwrap()
            .change,
        OwnInventoryChange::Unchanged,
    );
    // Poll the actual SDK through a cold verifier's asynchronous HTTP path.
    // Pending proves a suspended operation, not that inventory was sent or committed.
    let interrupt = Cancellation::default();
    let interrupted_original = request_until(&interrupt, deadline);
    let interrupted_caller = auth
        .issuer()
        .authenticate_runtime(&key, &interrupted_original)
        .unwrap();
    let interrupt_cache = inventory_verifier_cache();
    let mut reading = Box::pin(read_own_inventory(
        &client,
        &auth,
        &interrupted_caller,
        InventoryCredentialMode::BrokerNative(&interrupt_cache),
        &interrupted_original,
    ));
    let first_poll = std::future::poll_fn(|cx| {
        std::task::Poll::Ready(std::future::Future::poll(reading.as_mut(), cx))
    })
    .await;
    assert!(matches!(first_poll, std::task::Poll::Pending));
    interrupt.cancel();
    assert!(matches!(
        reading.await,
        Err(OwnInventoryReadFailure::Admission {
            source: InventoryAdmissionFailure::Auth(ServiceUseError::Broker(
                graphit_auth::broker::BrokerError::Cancelled
            )),
            effect: NetworkEffect::NotSent,
        }),
    ));
    let awaiting_deadline = Instant::now() + Duration::from_secs(2);
    let awaiting_original = request_until(&cancel, awaiting_deadline);
    let awaiting_caller = auth
        .issuer()
        .authenticate_runtime(&key, &awaiting_original)
        .unwrap();
    let deadline_cache = inventory_verifier_cache();
    let mut publishing = Box::pin(publish_maintained_inventory(
        &client,
        &auth,
        &awaiting_caller,
        InventoryCredentialMode::BrokerNative(&deadline_cache),
        &awaiting_original,
        65536,
    ));
    let first_poll = std::future::poll_fn(|cx| {
        std::task::Poll::Ready(std::future::Future::poll(publishing.as_mut(), cx))
    })
    .await;
    assert!(matches!(first_poll, std::task::Poll::Pending));
    tokio::time::sleep_until(tokio::time::Instant::from_std(awaiting_deadline)).await;
    assert!(matches!(
        publishing.await,
        Err(OwnInventoryPublicationFailure::Admission {
            source: InventoryAdmissionFailure::Auth(ServiceUseError::Broker(
                graphit_auth::broker::BrokerError::Deadline
            )),
            effect: NetworkEffect::NotSent,
        }),
    ));
    let aborted_cache = inventory_verifier_cache();
    let aborted_mode = InventoryCredentialMode::BrokerNative(&aborted_cache);
    let mut abandoned = Box::pin(publish_maintained_inventory(
        &client,
        &auth,
        &caller,
        aborted_mode,
        &original,
        65536,
    ));
    let first_poll = std::future::poll_fn(|cx| {
        std::task::Poll::Ready(std::future::Future::poll(abandoned.as_mut(), cx))
    })
    .await;
    assert!(matches!(first_poll, std::task::Poll::Pending));
    drop(abandoned);
    // Dropping the pending future releases its borrows; a new operation uses
    // the same real cache/owner, without assuming rollback or replaying the PUT.
    assert_eq!(
        read_own_inventory(&client, &auth, &caller, aborted_mode, &original)
            .await
            .unwrap()
            .projects,
        facts,
    );
    drop(awaiting_caller);
    drop(interrupted_caller);
    broker.command("deny_read").unwrap();
    remote_failure(
        read_own_inventory(&client, &auth, &caller, mode, &original).await,
        403,
        InventoryRemoteCode::Forbidden,
    );
    broker.command("allow_read").unwrap();
    assert!(
        read_own_inventory(&client, &auth, &caller, mode, &original)
            .await
            .is_ok()
    );
    broker.command("deny_publish").unwrap();
    publication_remote(
        publish_maintained_inventory(&client, &auth, &caller, mode, &original, 65536).await,
        403,
        InventoryPublicationRemoteCode::Forbidden,
    );
    broker.command("allow_publish").unwrap();
    clear_core_fixture(&temp, &paths, &original);
    assert_eq!(
        publish_maintained_inventory(&client, &auth, &caller, mode, &original, 65536)
            .await
            .unwrap()
            .change,
        OwnInventoryChange::Updated,
    );
    broker.command("delete_inventory").unwrap();
    broker.command("lose_next_successful_put_response").unwrap();
    let uncertain =
        publish_maintained_inventory(&client, &auth, &caller, mode, &original, 65536).await;
    assert!(matches!(
        uncertain,
        Err(OwnInventoryPublicationFailure::Network {
            effect: NetworkEffect::MayHaveApplied,
            ..
        }),
    ));
    // A new admitted GET observes the real committed row after transport delivery failed.
    assert!(
        read_own_inventory(&client, &auth, &caller, mode, &original)
            .await
            .unwrap()
            .projects
            .is_empty()
    );
    assert_eq!(
        publish_maintained_inventory(&client, &auth, &caller, mode, &original, 65536)
            .await
            .unwrap()
            .change,
        OwnInventoryChange::Unchanged,
    );
    broker
        .command_value(
            "replace_with_foreign_owner",
            &json!({"command":"replace_with_foreign_owner","subject":"fixture-foreign-owner"}),
        )
        .unwrap();
    publication_remote(
        publish_maintained_inventory(&client, &auth, &caller, mode, &original, 65536).await,
        409,
        InventoryPublicationRemoteCode::OwnerConflict,
    );
    let expired_at = Instant::now() + Duration::from_secs(2);
    let expired = request_until(&cancel, expired_at);
    let expired_caller = auth.issuer().authenticate_runtime(&key, &expired).unwrap();
    tokio::time::sleep_until(tokio::time::Instant::from_std(expired_at)).await;
    assert!(matches!(
        read_own_inventory(&client, &auth, &expired_caller, mode, &expired).await,
        Err(OwnInventoryReadFailure::Admission {
            source: InventoryAdmissionFailure::Scope(ref error),
            effect: NetworkEffect::NotSent,
        }) if error.kind() == io::ErrorKind::TimedOut,
    ));
    assert!(matches!(
        publish_maintained_inventory(&client, &auth, &expired_caller, mode, &expired, 65536).await,
        Err(OwnInventoryPublicationFailure::Admission {
            source: InventoryAdmissionFailure::Scope(ref error),
            effect: NetworkEffect::NotSent,
        }) if error.kind() == io::ErrorKind::TimedOut,
    ));
    broker.send(&json!({"command":"stats"})).unwrap();
    let stats: NativeStats = broker.receive().unwrap();
    assert_eq!(stats.event, "stats");
    assert!(stats.broker_metadata_requests > 0);
    assert!(stats.oidc_metadata_requests > 0);
    assert!(stats.jwks_requests > 0);
    assert_eq!(stats.token_requests, 0);
    assert_eq!(stats.userinfo_requests, 0);
    assert!(stats.put_success_responses >= 5);
    assert_eq!(stats.put_response_losses, 1);
    let session = auth.issuer().bootstrap_admin(&caller).unwrap();
    let parent = auth.issuer().admin_parent(&caller, &session).unwrap();
    auth.issuer().revoke_service_policy(&parent).unwrap();
    assert!(matches!(
        read_own_inventory(&client, &auth, &caller, mode, &original).await,
        Err(OwnInventoryReadFailure::Admission {
            source: InventoryAdmissionFailure::Auth(ServiceUseError::Auth(
                graphit_auth::AuthError::Denied
            )),
            effect: NetworkEffect::NotSent,
        }),
    ));
    assert!(matches!(
        publish_maintained_inventory(&client, &auth, &caller, mode, &original, 65536).await,
        Err(OwnInventoryPublicationFailure::Admission {
            source: InventoryAdmissionFailure::Auth(ServiceUseError::Auth(
                graphit_auth::AuthError::Denied
            )),
            effect: NetworkEffect::NotSent,
        }),
    ));
    cancel.cancel();
    assert!(matches!(
        read_own_inventory(&client, &auth, &caller, mode, &original).await,
        Err(OwnInventoryReadFailure::Admission {
            source: InventoryAdmissionFailure::Scope(ref error),
            effect: NetworkEffect::NotSent,
        }) if error.kind() == io::ErrorKind::Interrupted,
    ));
    assert!(matches!(
        publish_maintained_inventory(&client, &auth, &caller, mode, &original, 65536).await,
        Err(OwnInventoryPublicationFailure::Admission {
            source: InventoryAdmissionFailure::Scope(ref error),
            effect: NetworkEffect::NotSent,
        }) if error.kind() == io::ErrorKind::Interrupted,
    ));
    drop(expired_caller);
    drop(caller);
    drop(auth);
    broker
        .finish(true)
        .expect("public Native fixture cleanup failed");
}
fn remote_failure(
    result: Result<OwnInventory, OwnInventoryReadFailure>,
    status: u16,
    code: InventoryRemoteCode,
) {
    match result {
        Err(OwnInventoryReadFailure::Remote {
            status: actual,
            code: actual_code,
            effect,
        }) => {
            assert_eq!(actual, status);
            assert_eq!(actual_code, code);
            assert_eq!(effect, NetworkEffect::MayHaveApplied)
        }
        _ => panic!("expected finite Broker failure"),
    }
}
#[tokio::test]
#[ignore = "requires public prebuilt Broker fixture executable and native container runtime"]
async fn broker_inventory_native_process_journeys() {
    for empty in [false, true] {
        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(600);
        let original = request_until(&cancel, deadline);
        let temp = TemporaryInstallation::new(&original);
        let id = InstallationIdentityStore::new(&temp.paths)
            .unwrap()
            .ensure(Duration::from_secs(5), original.cancellation())
            .unwrap()
            .unit_id()
            .clone();
        let (paths, mut projects) = temp.projects(&original);
        if empty {
            projects.clear()
        }
        let mut broker =
            BrokerProcess::start(&id, &projects, deadline).expect("public fixture setup failed");
        let ready: Ready = broker.receive().expect("public fixture readiness failed");
        assert_eq!(ready.event, "ready");
        assert_eq!(ready.instance_id, id.as_str());
        assert_eq!(ready.projects.len(), projects.len());
        for (actual, expected) in ready.projects.iter().zip(&projects) {
            assert_eq!(actual.project_id, expected.project_id);
            assert_eq!(actual.versions, expected.versions)
        }
        assert!(
            !ready.audience.is_empty()
                && !ready.read_scopes.is_empty()
                && !ready.publish_scopes.is_empty()
        );
        assert!(
            ready.subject != ready.service_subject,
            "fixture identities must differ"
        );
        let auth =
            DaemonAuth::start(&temp.paths, &paths, startup(&ready.endpoint), &original).unwrap();
        let StoreOwner::Installation { installation_id } = auth.installation().live.owner() else {
            panic!("captured installation missing")
        };
        assert_eq!(installation_id, &id);
        profile(
            &auth,
            &temp.paths,
            &ready.endpoint,
            &ready.access_bearer,
            &ready.username,
            "access",
            &original,
        );
        let key = runtime_key(&auth, &original);
        let caller = auth.issuer().authenticate_runtime(&key, &original).unwrap();
        let client = network(&ready.endpoint, 65536);
        let snapshot = read_own_inventory(
            &client,
            &auth,
            &caller,
            InventoryCredentialMode::LocalBrokerKey,
            &original,
        )
        .await
        .unwrap();
        assert_eq!(snapshot.instance_id, id);
        assert_eq!(snapshot.projects, projects);
        assert!(snapshot.created_at >= 0 && snapshot.updated_at >= 0);
        let local = auth
            .issuer()
            .authenticate_local("runtime-inventory-local-key", &original)
            .unwrap();
        let local_snapshot = read_own_inventory(
            &client,
            &auth,
            &local,
            InventoryCredentialMode::LocalBrokerKey,
            &original,
        )
        .await
        .unwrap();
        assert_eq!(local_snapshot.instance_id, id);
        assert_eq!(local_snapshot.projects, projects);
        drop(local);
        let other_origin = network("http://127.0.0.1:1", 65536);
        assert!(matches!(
            read_own_inventory(
                &other_origin,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original
            )
            .await,
            Err(OwnInventoryReadFailure::Network {
                source: NetworkError::Origin,
                effect: NetworkEffect::NotSent
            })
        ));
        let small = network(&ready.endpoint, 1);
        assert!(matches!(
            read_own_inventory(
                &small,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original
            )
            .await,
            Err(OwnInventoryReadFailure::Network {
                source: NetworkError::Limit,
                effect: NetworkEffect::MayHaveApplied
            })
        ));
        broker.command("deny_read").unwrap();
        remote_failure(
            read_own_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original,
            )
            .await,
            403,
            InventoryRemoteCode::Forbidden,
        );
        broker.command("allow_read").unwrap();
        assert!(
            read_own_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original
            )
            .await
            .is_ok()
        );
        broker.command("revoke_access").unwrap();
        remote_failure(
            read_own_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original,
            )
            .await,
            401,
            InventoryRemoteCode::Unauthenticated,
        );
        broker.command("restore_access").unwrap();
        assert!(
            read_own_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original
            )
            .await
            .is_ok()
        );
        let other = request_until(&cancel, deadline);
        assert!(matches!(
            read_own_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &other
            )
            .await,
            Err(OwnInventoryReadFailure::Admission {
                source: InventoryAdmissionFailure::DifferentOriginal,
                effect: NetworkEffect::NotSent
            })
        ));
        profile(
            &auth,
            &temp.paths,
            &ready.endpoint,
            &ready.service_bearer,
            &ready.service_username,
            "service",
            &original,
        );
        remote_failure(
            read_own_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original,
            )
            .await,
            409,
            InventoryRemoteCode::OwnerConflict,
        );
        broker.command("delete_inventory").unwrap();
        remote_failure(
            read_own_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original,
            )
            .await,
            404,
            InventoryRemoteCode::NotFound,
        );
        let moved = temp.directory.path().join("moved-installation");
        std::fs::rename(temp.paths.root(), &moved).unwrap();
        let physical = read_own_inventory(
            &client,
            &auth,
            &caller,
            InventoryCredentialMode::LocalBrokerKey,
            &original,
        )
        .await;
        std::fs::rename(&moved, temp.paths.root()).unwrap();
        assert!(matches!(
            physical,
            Err(OwnInventoryReadFailure::Admission {
                source: InventoryAdmissionFailure::Scope(_),
                effect: NetworkEffect::NotSent
            })
        ));
        let expiry = Instant::now() + Duration::from_secs(2);
        let expired = request_until(&cancel, expiry);
        let expired_caller = auth.issuer().authenticate_runtime(&key, &expired).unwrap();
        tokio::time::sleep_until(tokio::time::Instant::from_std(expiry)).await;
        assert!(
            matches!(read_own_inventory(&client,&auth,&expired_caller, InventoryCredentialMode::LocalBrokerKey,&expired).await,Err(OwnInventoryReadFailure::Admission {source:InventoryAdmissionFailure::Scope(ref error),effect:NetworkEffect::NotSent}) if error.kind()==io::ErrorKind::TimedOut)
        );
        drop(expired_caller);
        let session = auth.issuer().bootstrap_admin(&caller).unwrap();
        let parent = auth.issuer().admin_parent(&caller, &session).unwrap();
        auth.issuer().revoke_service_policy(&parent).unwrap();
        assert!(matches!(
            read_own_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original
            )
            .await,
            Err(OwnInventoryReadFailure::Admission {
                source: InventoryAdmissionFailure::Auth(ServiceUseError::Auth(
                    graphit_auth::AuthError::Denied
                )),
                effect: NetworkEffect::NotSent
            })
        ));
        cancel.cancel();
        assert!(matches!(
            read_own_inventory(&client, &auth, &caller, InventoryCredentialMode::LocalBrokerKey, &original).await,
            Err(OwnInventoryReadFailure::Admission {
                source: InventoryAdmissionFailure::Scope(ref error),
                effect: NetworkEffect::NotSent,
            }) if error.kind() == io::ErrorKind::Interrupted
        ));
        drop(caller);
        drop(auth);
        broker
            .finish(!empty)
            .expect("public fixture cleanup failed");
    }
}

#[test]
fn broker_inventory_publication_rejects_status_shape_and_timestamp_mismatches() {
    let id = InstallationId::new(INSTANCE).unwrap();
    let valid =
        json!({"instance_id":INSTANCE,"change":"created","created_at":1234567,"updated_at":0});
    for (status, change) in [(201, "created"), (200, "updated"), (200, "unchanged")] {
        let mut wire = valid.clone();
        wire["change"] = json!(change);
        assert!(decode_publication(&serde_json::to_vec(&wire).unwrap(), &id, status).is_ok());
        assert!(
            decode_publication(
                &serde_json::to_vec(&wire).unwrap(),
                &id,
                if status == 201 { 200 } else { 201 }
            )
            .is_err()
        );
    }
    let mut cases = Vec::new();
    for value in [
        json!("other"),
        json!(" fixture-installation"),
        json!("fixture-installation "),
        json!(""),
    ] {
        let mut wire = valid.clone();
        wire["instance_id"] = value;
        cases.push(wire);
    }
    for field in ["created_at", "updated_at"] {
        for value in [
            json!(-1),
            json!(1.5),
            json!("1"),
            Value::Null,
            json!(18446744073709551615u64),
        ] {
            let mut wire = valid.clone();
            wire[field] = value;
            cases.push(wire);
        }
    }
    for field in ["instance_id", "change", "created_at", "updated_at"] {
        let mut wire = valid.clone();
        wire.as_object_mut().unwrap().remove(field);
        cases.push(wire);
    }
    for change in ["unknown", "Created", ""] {
        let mut wire = valid.clone();
        wire["change"] = json!(change);
        cases.push(wire);
    }
    let mut extra = valid.clone();
    extra["projects"] = json!([]);
    cases.push(extra);
    for (index, wire) in cases.iter().enumerate() {
        assert!(
            decode_publication(&serde_json::to_vec(wire).unwrap(), &id, 201).is_err(),
            "invalid case {index}"
        );
    }
    for raw in [
        r#"{"instance_id":"fixture-installation","change":"created","change":"created","created_at":1,"updated_at":1}"#,
        r#"{"instance_id":"fixture-installation","change":"created","created_at":1,"updated_at":1} {}"#,
    ] {
        assert!(decode_publication(raw.as_bytes(), &id, 201).is_err());
    }
}
#[test]
fn broker_inventory_publication_codes_are_finite_and_match_put_status() {
    for (status, code) in [
        (400, "invalid_json"),
        (400, "invalid_inventory"),
        (400, "body_too_large"),
        (401, "unauthenticated"),
        (403, "forbidden"),
        (409, "owner_conflict"),
        (404, "not_found"),
        (503, "authentication_unavailable"),
        (503, "authorization_unavailable"),
        (503, "invalid_policy"),
        (503, "registration_unavailable"),
        (503, "closed"),
        (503, "outcome_uncertain"),
        (504, "deadline"),
        (405, "method_not_allowed"),
    ] {
        let wire = serde_json::to_vec(&json!({"error":{"code":code}})).unwrap();
        assert!(decode_publication_remote(&wire, status).is_ok());
        assert!(decode_publication_remote(&wire, 200).is_err());
    }
    for raw in [
        r#"{"error":{"code":"inventory_unavailable"}}"#,
        r#"{"error":{"code":"unknown"}}"#,
        r#"{"error":{"code":"forbidden","private":"data"}}"#,
        r#"{"error":{"code":"forbidden","code":"forbidden"}}"#,
        r#"{"error":{"code":"forbidden"}} {}"#,
    ] {
        assert!(decode_publication_remote(raw.as_bytes(), 503).is_err());
    }
}
#[test]
fn broker_inventory_publication_encoding_uses_actual_client_bound_and_preserves_labels() {
    let cancellation = Cancellation::default();
    let original = request_until(&cancellation, Instant::now() + Duration::from_secs(5));
    let projects = vec![MaintainedProjectVersions {
        project_id: PROJECT.parse().unwrap(),
        versions: vec!["main".into(), "feature/topic".into(), "versão\"nova".into()],
    }];
    let expected=serde_json::to_vec(&json!({"projects":[{"project_id":PROJECT,"versions":["main","feature/topic","versão\"nova"]}]})).unwrap();
    let client = network_with_request_bound("http://127.0.0.1:1", 65536, expected.len());
    let encoded = encode_inventory(&projects, &client, &original).unwrap();
    assert_eq!(encoded, expected);
    let small = network_with_request_bound("http://127.0.0.1:1", 65536, expected.len() - 1);
    let failure = encode_inventory(&projects, &small, &original).unwrap_err();
    assert_eq!(failure.io_error_kind(), Some(io::ErrorKind::FileTooLarge));
    let empty = network_with_request_bound("http://127.0.0.1:1", 65536, b"{\"projects\":[]}".len());
    assert_eq!(
        encode_inventory(&[], &empty, &original).unwrap(),
        b"{\"projects\":[]}"
    );
    cancellation.cancel();
    assert_eq!(
        encode_inventory(&projects, &client, &original)
            .unwrap_err()
            .io_error_kind(),
        Some(io::ErrorKind::Interrupted)
    );
}
fn inventory_capture(
    project: &ProjectPaths,
    selected: &str,
    original: &RequestContext<'_>,
) -> CapturedProjectInventory {
    let checkout = GitCheckout::inspect(
        project.root(),
        Duration::from_secs(5),
        original.cancellation(),
    )
    .unwrap();
    let selection =
        VersionContext::select(project, checkout.as_checkout(), selected, None, "").unwrap();
    CapturedProjectInventory::capture(
        project,
        &checkout,
        &selection,
        scopes(),
        NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap(),
        original.snapshot(),
    )
    .unwrap()
}
fn maintain_project(
    temp: &TemporaryInstallation,
    project: &ProjectPaths,
    selected: &str,
    retained: &[&str],
    original: &RequestContext<'_>,
) {
    let capture = inventory_capture(project, selected, original);
    let installation = installation_bootstrap::prepare(
        &temp.paths,
        BootstrapLimits::new(65536, 256).unwrap(),
        original.snapshot(),
    )
    .into_installation()
    .unwrap();
    let store = GlobalProjectInventory::at(&temp.paths, 65536).unwrap();
    let mut writer = store.writer(installation, original.snapshot()).unwrap();
    writer.register_checkout(&capture).unwrap();
    for version in retained {
        writer.retain_version(&capture, version).unwrap();
    }
}
fn core_projects(
    temp: &TemporaryInstallation,
    original: &RequestContext<'_>,
) -> Vec<MaintainedProjectVersions> {
    let inventory = GlobalProjectInventory::at(&temp.paths, 65536)
        .unwrap()
        .load(original.snapshot())
        .unwrap();
    inventory
        .projects()
        .map(|project| MaintainedProjectVersions {
            project_id: project.identity().id(),
            versions: project.versions().map(str::to_owned).collect(),
        })
        .collect()
}
fn clear_core_fixture(
    temp: &TemporaryInstallation,
    paths: &[ProjectPaths],
    original: &RequestContext<'_>,
) {
    let projects = core_projects(temp, original);
    let installation = installation_bootstrap::prepare(
        &temp.paths,
        BootstrapLimits::new(65536, 256).unwrap(),
        original.snapshot(),
    )
    .into_installation()
    .unwrap();
    let store = GlobalProjectInventory::at(&temp.paths, 65536).unwrap();
    let mut writer = store.writer(installation, original.snapshot()).unwrap();
    for path in paths {
        let id = ProjectIdentityStore::at(path).load().unwrap().unwrap().id();
        writer.remove_checkout(id, path.root()).unwrap();
    }
    for project in projects {
        for version in project.versions {
            writer.remove_version(project.project_id, &version).unwrap();
        }
    }
}
fn publication_remote(
    result: Result<OwnInventoryPublication, OwnInventoryPublicationFailure>,
    status: u16,
    code: InventoryPublicationRemoteCode,
) {
    match result {
        Err(OwnInventoryPublicationFailure::Remote {
            status: actual,
            code: actual_code,
            effect,
        }) => {
            assert_eq!(actual, status);
            assert_eq!(actual_code, code);
            assert_eq!(effect, NetworkEffect::MayHaveApplied);
        }
        _ => panic!("expected finite publication failure"),
    }
}
#[tokio::test]
#[ignore = "requires public prebuilt Broker fixture executable and native container runtime"]
async fn broker_inventory_native_publication_journeys() {
    for empty in [false, true] {
        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(600);
        let original = request_until(&cancel, deadline);
        let temp = TemporaryInstallation::new(&original);
        let id = InstallationIdentityStore::new(&temp.paths)
            .unwrap()
            .ensure(Duration::from_secs(5), original.cancellation())
            .unwrap()
            .unit_id()
            .clone();
        let (paths, _) = temp.projects(&original);
        if !empty {
            maintain_project(&temp, &paths[0], "", &[], &original);
            maintain_project(&temp, &paths[1], "feature/topic", &[], &original);
        }
        let projects = core_projects(&temp, &original);
        let mut broker =
            BrokerProcess::start(&id, &projects, deadline).expect("public fixture setup failed");
        let ready: Ready = broker.receive().expect("public fixture readiness failed");
        assert_eq!(ready.event, "ready");
        assert_eq!(ready.instance_id, id.as_str());
        assert!(
            ready.subject != ready.service_subject,
            "fixture identities must differ"
        );
        let auth = DaemonAuth::start(
            &temp.paths,
            &paths,
            startup_with_purposes(
                &ready.endpoint,
                &[
                    ServicePurpose::InventoryRead,
                    ServicePurpose::InventoryPublish,
                ],
            ),
            &original,
        )
        .unwrap();
        profile(
            &auth,
            &temp.paths,
            &ready.endpoint,
            &ready.access_bearer,
            &ready.username,
            "access",
            &original,
        );
        let key = runtime_key(&auth, &original);
        let caller = auth.issuer().authenticate_runtime(&key, &original).unwrap();
        let client = network(&ready.endpoint, 65536);
        broker.command("delete_inventory").unwrap();
        let first = publish_maintained_inventory(
            &client,
            &auth,
            &caller,
            InventoryCredentialMode::LocalBrokerKey,
            &original,
            65536,
        )
        .await
        .unwrap();
        assert_eq!(first.instance_id, id);
        assert_eq!(first.change, OwnInventoryChange::Created);
        let second = publish_maintained_inventory(
            &client,
            &auth,
            &caller,
            InventoryCredentialMode::LocalBrokerKey,
            &original,
            65536,
        )
        .await
        .unwrap();
        assert_eq!(second.change, OwnInventoryChange::Unchanged);
        assert_eq!(
            (second.created_at, second.updated_at),
            (first.created_at, first.updated_at)
        );
        let local = auth
            .issuer()
            .authenticate_local("runtime-inventory-local-key", &original)
            .unwrap();
        let local_result = publish_maintained_inventory(
            &client,
            &auth,
            &local,
            InventoryCredentialMode::LocalBrokerKey,
            &original,
            65536,
        )
        .await
        .unwrap();
        assert_eq!(local_result.change, OwnInventoryChange::Unchanged);
        drop(local);
        let snapshot = read_own_inventory(
            &client,
            &auth,
            &caller,
            InventoryCredentialMode::LocalBrokerKey,
            &original,
        )
        .await
        .unwrap();
        assert_eq!(snapshot.projects, projects);
        let tiny = network_with_request_bound(&ready.endpoint, 65536, 1);
        assert!(
            matches!(publish_maintained_inventory(&tiny,&auth,&caller, InventoryCredentialMode::LocalBrokerKey,&original,65536).await,
            Err(OwnInventoryPublicationFailure::Encoding{ref source,effect:NetworkEffect::NotSent}) if source.io_error_kind()==Some(io::ErrorKind::FileTooLarge))
        );
        let after = read_own_inventory(
            &client,
            &auth,
            &caller,
            InventoryCredentialMode::LocalBrokerKey,
            &original,
        )
        .await
        .unwrap();
        assert_eq!(after, snapshot);
        if !empty {
            assert!(matches!(
                publish_maintained_inventory(
                    &client,
                    &auth,
                    &caller,
                    InventoryCredentialMode::LocalBrokerKey,
                    &original,
                    1
                )
                .await,
                Err(OwnInventoryPublicationFailure::Snapshot {
                    effect: NetworkEffect::NotSent,
                    ..
                })
            ));
            maintain_project(&temp, &paths[0], "", &["versão"], &original);
            let changed = publish_maintained_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original,
                65536,
            )
            .await
            .unwrap();
            assert_eq!(changed.change, OwnInventoryChange::Updated);
            assert_eq!(changed.created_at, first.created_at);
            assert_eq!(
                read_own_inventory(
                    &client,
                    &auth,
                    &caller,
                    InventoryCredentialMode::LocalBrokerKey,
                    &original
                )
                .await
                .unwrap()
                .projects,
                core_projects(&temp, &original)
            );
            let historical_id = ProjectIdentityStore::at(&paths[0])
                .load()
                .unwrap()
                .unwrap()
                .id();
            let facts = core_projects(&temp, &original);
            let moved_checkout = temp.directory.path().join("historical-checkout");
            std::fs::rename(paths[0].root(), &moved_checkout).unwrap();
            let scope_context = ScopeContext::new(deadline, cancel.clone());
            assert!(
                auth.installation()
                    .live
                    .revalidate_bounded(&scope_context)
                    .is_err()
            );
            assert_eq!(
                read_maintained_inventory(&auth, original.snapshot(), 65536)
                    .unwrap()
                    .projects,
                facts
            );
            let local = auth
                .issuer()
                .authenticate_local("runtime-inventory-local-key", &original)
                .unwrap();
            for publisher in [&caller, &local] {
                assert_eq!(
                    publish_maintained_inventory(
                        &client,
                        &auth,
                        publisher,
                        InventoryCredentialMode::LocalBrokerKey,
                        &original,
                        65536
                    )
                    .await
                    .unwrap()
                    .change,
                    OwnInventoryChange::Unchanged
                );
                assert_eq!(
                    read_own_inventory(
                        &client,
                        &auth,
                        publisher,
                        InventoryCredentialMode::LocalBrokerKey,
                        &original
                    )
                    .await
                    .unwrap()
                    .projects,
                    facts
                );
            }
            let installation = installation_bootstrap::prepare(
                &temp.paths,
                BootstrapLimits::new(65536, 256).unwrap(),
                original.snapshot(),
            )
            .into_installation()
            .unwrap();
            let store = GlobalProjectInventory::at(&temp.paths, 65536).unwrap();
            assert!(
                store
                    .writer(installation, original.snapshot())
                    .unwrap()
                    .remove_checkout(historical_id, paths[0].root())
                    .unwrap()
            );
            let metadata = store.load(original.snapshot()).unwrap();
            let entry = metadata.get(historical_id).unwrap();
            assert_eq!(entry.checkouts().len(), 0);
            assert!(
                entry.versions().any(|version| version == "versão"),
                "checkout retirement preserves maintained versions"
            );
            for publisher in [&caller, &local] {
                assert_eq!(
                    publish_maintained_inventory(
                        &client,
                        &auth,
                        publisher,
                        InventoryCredentialMode::LocalBrokerKey,
                        &original,
                        65536
                    )
                    .await
                    .unwrap()
                    .change,
                    OwnInventoryChange::Unchanged
                );
                assert_eq!(
                    read_own_inventory(
                        &client,
                        &auth,
                        publisher,
                        InventoryCredentialMode::LocalBrokerKey,
                        &original
                    )
                    .await
                    .unwrap()
                    .projects,
                    facts
                );
            }
            drop(local);
            assert!(
                auth.installation()
                    .live
                    .revalidate_bounded(&scope_context)
                    .is_err()
            );
            assert!(!paths[0].root().exists());
            assert!(paths[1].root().exists());
            std::fs::rename(&moved_checkout, paths[0].root()).unwrap();
            clear_core_fixture(&temp, &paths, &original);
            let cleared = publish_maintained_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original,
                65536,
            )
            .await
            .unwrap();
            assert_eq!(cleared.change, OwnInventoryChange::Updated);
            assert_eq!(cleared.created_at, first.created_at);
            assert!(
                read_own_inventory(
                    &client,
                    &auth,
                    &caller,
                    InventoryCredentialMode::LocalBrokerKey,
                    &original
                )
                .await
                .unwrap()
                .projects
                .is_empty()
            );
        }
        let small_response = network(&ready.endpoint, 1);
        assert!(matches!(
            publish_maintained_inventory(
                &small_response,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original,
                65536
            )
            .await,
            Err(OwnInventoryPublicationFailure::Network {
                source: NetworkError::Limit,
                effect: NetworkEffect::MayHaveApplied
            })
        ));
        let other_origin = network("http://127.0.0.1:1", 65536);
        assert!(matches!(
            publish_maintained_inventory(
                &other_origin,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original,
                65536
            )
            .await,
            Err(OwnInventoryPublicationFailure::Network {
                source: NetworkError::Origin,
                effect: NetworkEffect::NotSent
            })
        ));
        broker.command("revoke_access").unwrap();
        publication_remote(
            publish_maintained_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original,
                65536,
            )
            .await,
            401,
            InventoryPublicationRemoteCode::Unauthenticated,
        );
        broker.command("restore_access").unwrap();
        assert_eq!(
            publish_maintained_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original,
                65536
            )
            .await
            .unwrap()
            .change,
            OwnInventoryChange::Unchanged
        );
        profile(
            &auth,
            &temp.paths,
            &ready.endpoint,
            &ready.service_bearer,
            &ready.service_username,
            "service",
            &original,
        );
        publication_remote(
            publish_maintained_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original,
                65536,
            )
            .await,
            409,
            InventoryPublicationRemoteCode::OwnerConflict,
        );
        profile(
            &auth,
            &temp.paths,
            &ready.endpoint,
            &ready.access_bearer,
            &ready.username,
            "restored-access",
            &original,
        );
        let other = request_until(&cancel, deadline);
        assert!(matches!(
            publish_maintained_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &other,
                65536
            )
            .await,
            Err(OwnInventoryPublicationFailure::Admission {
                source: InventoryAdmissionFailure::DifferentOriginal,
                effect: NetworkEffect::NotSent
            })
        ));
        let moved = temp.directory.path().join("moved-installation");
        std::fs::rename(temp.paths.root(), &moved).unwrap();
        let physical = publish_maintained_inventory(
            &client,
            &auth,
            &caller,
            InventoryCredentialMode::LocalBrokerKey,
            &original,
            65536,
        )
        .await;
        std::fs::rename(&moved, temp.paths.root()).unwrap();
        assert!(matches!(
            physical,
            Err(OwnInventoryPublicationFailure::Admission {
                source: InventoryAdmissionFailure::Scope(_),
                effect: NetworkEffect::NotSent
            })
        ));
        let expiry = Instant::now() + Duration::from_secs(2);
        let expired = request_until(&cancel, expiry);
        let expired_caller = auth.issuer().authenticate_runtime(&key, &expired).unwrap();
        tokio::time::sleep_until(expiry.into()).await;
        assert!(
            matches!(publish_maintained_inventory(&client,&auth,&expired_caller, InventoryCredentialMode::LocalBrokerKey,&expired,65536).await,
            Err(OwnInventoryPublicationFailure::Admission{source:InventoryAdmissionFailure::Scope(ref error),effect:NetworkEffect::NotSent}) if error.kind()==io::ErrorKind::TimedOut)
        );
        drop(expired_caller);
        let session = auth.issuer().bootstrap_admin(&caller).unwrap();
        let parent = auth.issuer().admin_parent(&caller, &session).unwrap();
        auth.issuer().revoke_service_policy(&parent).unwrap();
        assert!(matches!(
            publish_maintained_inventory(
                &client,
                &auth,
                &caller,
                InventoryCredentialMode::LocalBrokerKey,
                &original,
                65536
            )
            .await,
            Err(OwnInventoryPublicationFailure::Admission {
                source: InventoryAdmissionFailure::Auth(ServiceUseError::Auth(
                    graphit_auth::AuthError::Denied
                )),
                effect: NetworkEffect::NotSent
            })
        ));
        cancel.cancel();
        assert!(
            matches!(publish_maintained_inventory(&client,&auth,&caller, InventoryCredentialMode::LocalBrokerKey,&original,65536).await,
            Err(OwnInventoryPublicationFailure::Admission{source:InventoryAdmissionFailure::Scope(ref error),effect:NetworkEffect::NotSent}) if error.kind()==io::ErrorKind::Interrupted)
        );
        drop(caller);
        drop(auth);
        broker
            .finish(!empty)
            .expect("public fixture cleanup failed");
    }
}
