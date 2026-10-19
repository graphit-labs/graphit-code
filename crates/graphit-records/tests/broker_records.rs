#![cfg(target_os = "linux")]

#[path = "task_store/fixture.rs"]
pub mod fixture;

use fixture::{Fixture, read_limits, request_until, write_limits};
use futures::FutureExt;
use graphit_auth::{
    ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, RoleIssuer,
    broker::{
        BrokerPolicy, BrokerRequestContext, BrokerVerifier, JsonLimits, VerifierLimits,
        project_access::{
            BrokerProjectAccess, BrokerProjectCaller, BrokerProjectError, BrokerProjectScope,
            BrokerRecordRead, ProjectAccessPolicy,
        },
    },
    profile::{AiService, BrokerService, ProviderConfig, ProviderKind, S3Location, S3Source},
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    file_lock::Cancellation,
    installation_identity::InstallationId,
    project_paths::InstallationPaths,
};
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use graphit_records::task_store::{CreateSession, NativeTaskStore, TaskStoreError};
use serde_json::{Value, json};
use std::{
    panic::AssertUnwindSafe,
    process::Stdio,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    process::{Child, ChildStdin, ChildStdout},
    task::JoinHandle,
};

fn json_limits() -> JsonLimits {
    JsonLimits {
        bytes: 8192,
        nodes: 256,
        depth: 16,
        scalar_bytes: 2048,
        collection_entries: 32,
    }
}

fn verifier_limits() -> VerifierLimits {
    VerifierLimits {
        token_bytes: 8192,
        json: json_limits(),
        max_keys: 8,
        metadata_ttl: Duration::from_secs(300),
        key_ttl: Duration::from_secs(300),
        unknown_key_cooldown: Duration::from_millis(100),
    }
}

fn network_limits() -> NetworkLimits {
    NetworkLimits {
        request_body_bytes: 8192,
        response_body_bytes: 8192,
        url_bytes: 2048,
        header_bytes: 16384,
        header_count: 32,
        max_in_flight: 8,
        max_idle_per_host: 2,
        idle_timeout: Duration::from_secs(30),
        max_certificates: 2,
        max_certificate_bytes: 8192,
    }
}

fn network(origin: &str) -> ClientPolicy<'_> {
    ClientPolicy {
        origin,
        allow_loopback_http: true,
        proxy: None,
        additional_roots: &[],
    }
}

// Only discovery and the qualified fixture's public signing keys are served here.
// The PROJECT authorization response always comes from the actual Broker SQL endpoint.
struct Discovery {
    origin: String,
    resource: String,
    keys: Arc<Mutex<Value>>,
    task: JoinHandle<()>,
}
impl Discovery {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let resource = format!("{origin}/mcp");
        let broker = json!({"version":"1","issuer":origin,"authentication":{
            "type":"oidc","issuer":origin,"client_id":"records-client",
            "scopes":["openid","profile"],"redirect_uri_path":"/callback",
            "audiences":["records-api"],"access_token_audience":"records-api",
            "mcp_resources":[resource]}});
        let standard = json!({"issuer":origin,
            "authorization_endpoint":format!("{origin}/authorize"),
            "token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/keys"),
            "grant_types_supported":["authorization_code","refresh_token"],
            "code_challenge_methods_supported":["S256"],
            "token_endpoint_auth_methods_supported":["none"],
            "id_token_signing_alg_values_supported":["EdDSA"]});
        let keys = Arc::new(Mutex::new(Value::Null));
        let server_keys = Arc::clone(&keys);
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut input = Vec::new();
                let mut block = [0; 1024];
                while !input.windows(4).any(|value| value == b"\r\n\r\n") {
                    let Ok(Ok(read)) =
                        tokio::time::timeout(Duration::from_secs(10), stream.read(&mut block))
                            .await
                    else {
                        break;
                    };
                    if read == 0 || input.len() + read > 8192 {
                        break;
                    }
                    input.extend_from_slice(&block[..read]);
                }
                let request = String::from_utf8_lossy(&input);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1));
                let (status, body) = match path {
                    Some("/.well-known/graphit-broker") => (200, broker.to_string()),
                    Some("/.well-known/openid-configuration") => (200, standard.to_string()),
                    Some("/keys") => (200, server_keys.lock().unwrap().to_string()),
                    _ => (404, "{}".into()),
                };
                let response = format!(
                    "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        Self {
            origin,
            resource,
            keys,
            task,
        }
    }
}
impl Drop for Discovery {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct BrokerProcess {
    process: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    ready: bool,
}
impl BrokerProcess {
    fn spawn() -> Self {
        let binary = std::env::var_os("GRAPHIT_BROKER_PROJECT_FIXTURE_BIN")
            .expect("qualified public Broker fixture executable");
        let mut process = tokio::process::Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let input = process.stdin.take();
        let output = BufReader::new(process.stdout.take().unwrap());
        Self {
            process,
            input,
            output,
            ready: false,
        }
    }
    async fn command(&mut self, value: Value) -> Value {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        let input = self.input.as_mut().unwrap();
        input.write_all(&bytes).await.unwrap();
        input.flush().await.unwrap();
        let mut line = String::new();
        let read = tokio::time::timeout(Duration::from_secs(180), self.output.read_line(&mut line))
            .await
            .expect("public Broker response deadline")
            .unwrap();
        assert_ne!(read, 0, "public Broker response closed");
        // The private Ready bearer is parsed only in memory and never printed.
        serde_json::from_str(&line).expect("public Broker response shape")
    }
    async fn finish(mut self) {
        if self.ready {
            assert_eq!(
                self.command(json!({"command":"shutdown"})).await["ok"],
                true
            );
        }
        drop(self.input.take());
        assert!(
            tokio::time::timeout(Duration::from_secs(60), self.process.wait())
                .await
                .expect("public Broker cleanup deadline")
                .unwrap()
                .success()
        );
    }
}

fn auth_store(fixture: &Fixture, original: &graphit_hub::policy::RequestContext<'_>) -> AuthStore {
    let paths = InstallationPaths::resolve(
        fixture.installation.brand().clone(),
        &fixture.root,
        None,
        Some(fixture.installation.owner_root()),
    )
    .unwrap();
    let scope = CapturedPublicationScope::installation(
        &paths,
        InstallationId::new("native-task-test").unwrap(),
        [fixture.installation.owner_root().to_owned()],
        ScopeLimits::new(16, 4096, 128).unwrap(),
        original.snapshot(),
    )
    .unwrap();
    let physical = AuthDocumentStore::capture(
        scope,
        AuthDocumentLimits::new(4096, 65536, 256, 65536).unwrap(),
        original.snapshot(),
    )
    .unwrap();
    AuthStore::new(
        physical,
        graphit_auth::profile::ProfileLimits {
            bytes: 4096,
            nodes: 4096,
            scalar_bytes: 256,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
        original,
    )
    .unwrap()
}

fn add_provider(
    store: &AuthStore,
    original: &graphit_hub::policy::RequestContext<'_>,
    discovery: &Discovery,
) -> u64 {
    let snapshot = store.load(original).unwrap();
    let revision = snapshot.document().revision();
    let _ = store
        .mutate(snapshot, original, |document, request| {
            document.add_provider(
                "broker".into(),
                ProviderConfig {
                    kind: ProviderKind::Broker(BrokerService {
                        endpoint: discovery.origin.clone(),
                        mcp_resource: Some(discovery.resource.clone()),
                        allow_anonymous: false,
                    }),
                    embedding: AiService::Disabled,
                    rerank: AiService::Disabled,
                    s3: S3Location {
                        bucket: None,
                        region: None,
                        endpoint: None,
                        prefix: None,
                        source: S3Source::BrokerScope,
                    },
                },
                revision,
                1,
                request,
            )
        })
        .unwrap();
    store
        .load(original)
        .unwrap()
        .document()
        .provider("broker")
        .unwrap()
        .revision()
}

fn broker_scope(fixture: &Fixture) -> BrokerProjectScope<'_> {
    let scope = fixture.scope();
    BrokerProjectScope {
        installation: scope.installation,
        project: scope.project.unwrap(),
        providers: &[],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Docker and the qualified public Broker PostgreSQL/TLS PROJECT fixture binary"]
// This fixture proves actual Broker PROJECT ACL, Original binding and module checks.
// It does not compose RecordStorage discovery/STS or qualify successful corpus reads;
// those belong to App consumers using the production record-storage resolver.
async fn broker_task_project_acl_preserves_original_and_denies_before_storage_io() {
    let mut discovery = Discovery::start().await;
    let cancel = Cancellation::default();
    let original = request_until(&cancel, Instant::now() + Duration::from_secs(240));
    let fixture = Fixture::new(&original, "main");
    let issuer: RoleIssuer = fixture.issuer(&original);
    let store = auth_store(&fixture, &original);
    let revision = add_provider(&store, &original, &discovery);
    let (bearer, reference, _run) = fixture.issue(&issuer, &original);
    let grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &reference,
            fixture.scope(),
            &original,
        )
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_write(&grant, &original, ProjectWriteOperation::Task)
        .unwrap();
    // A genuine local corpus is only the sentinel for checks that must deny before IO.
    let session = NativeTaskStore::create_session(
        original.snapshot(),
        &write,
        CreateSession {
            idempotency_key: "broker-read".into(),
            title: "Broker-visible session".into(),
            description: "Persisted through legitimate Task write authority".into(),
            strategy: "Read through independently verified Broker access".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();

    let mut broker = BrokerProcess::spawn();
    let result = AssertUnwindSafe(async {
        let ready = broker
            .command(json!({"command":"start","issuer":discovery.origin,
            "api_audience":"records-api","resource":discovery.resource,"client_id":"records-client",
            "subject":"records-subject","scope":"","project_ids":["01KSH1CRFFG8Z74B5ZS78WW808"],
            "access_ttl_seconds":600}))
            .await;
        assert_eq!(ready["event"], "ready");
        broker.ready = true;
        *discovery.keys.lock().unwrap() = ready["trusted_jwks"].clone();
        let verifier = BrokerVerifier::new(
            BrokerPolicy {
                endpoint: &discovery.origin,
                mcp_resource: Some(&discovery.resource),
                provider_revision: revision,
                network: network(&discovery.origin),
            },
            network_limits(),
            verifier_limits(),
        )
        .unwrap();
        let caller = verifier
            .verify(
                ready["access_bearer"].as_str().unwrap(),
                BrokerRequestContext::new(original.snapshot()),
            )
            .await
            .unwrap();
        let endpoint = ready["authorize_endpoint"].as_str().unwrap();
        let access = BrokerProjectAccess::new(
            ProjectAccessPolicy {
                endpoint,
                network: network(endpoint),
            },
            network_limits(),
            json_limits(),
        )
        .unwrap();
        let input = |module| BrokerRecordRead {
            caller: BrokerProjectCaller::Incoming(&caller),
            verifier: &verifier,
            backend: &access,
            original: &original,
            scope: broker_scope(&fixture),
            provider: "broker",
            module,
        };
        let read = authority
            .admit_broker_read(input(ProjectReadModule::Task))
            .await
            .unwrap();
        assert!(std::ptr::eq(read.request().unwrap(), &original));
        assert_eq!(read.project().unwrap().version.version(), "main");
        assert_eq!(read.module(), ProjectReadModule::Task);
        read.recheck().unwrap();

        let memory_read = authority
            .admit_broker_read(input(ProjectReadModule::Memory))
            .await
            .unwrap();
        let foreign = fixture::request(&cancel);
        assert!(matches!(
            NativeTaskStore::read_session(
                foreign.snapshot(),
                &read,
                graphit_records::task_store::SessionGetInput::current(session.id),
                read_limits()
            )
            .await,
            Err(TaskStoreError::DifferentRequest)
        ));
        let mut foreign_input = input(ProjectReadModule::Task);
        foreign_input.original = &foreign;
        assert!(authority.admit_broker_read(foreign_input).await.is_err());

        // Invalid storage cannot mask admission denial or trigger a disk fallback.
        let index = fixture.project.runtime_root().join("task/index.lance");
        std::fs::rename(&index, index.with_extension("saved")).unwrap();
        std::fs::write(&index, b"not a dataset").unwrap();
        assert!(matches!(
            NativeTaskStore::read_session(
                original.snapshot(),
                &memory_read,
                graphit_records::task_store::SessionGetInput::current(session.id),
                read_limits()
            )
            .await,
            Err(TaskStoreError::WrongAdmission)
        ));
        assert_eq!(
            broker.command(json!({"command":"deny_access"})).await["ok"],
            true
        );
        assert!(matches!(
            authority
                .admit_broker_read(input(ProjectReadModule::Task))
                .await,
            Err(BrokerProjectError::Denied)
        ));
        // The existing proof only rechecks local facts. A new operation needs fresh ACL.
        read.recheck().unwrap();
    })
    .catch_unwind()
    .await;
    broker.finish().await;
    discovery.task.abort();
    let _ = (&mut discovery.task).await;
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
