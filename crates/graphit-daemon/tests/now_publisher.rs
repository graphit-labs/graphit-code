#![cfg(target_os = "linux")]

#[expect(
    dead_code,
    reason = "Mature PostgreSQL support also serves other native Broker control journeys"
)]
#[path = "../../../../graphit-broker/crates/broker-control/tests/support/mod.rs"]
mod pg;

use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use broker_control::{
    BrokerAccessVerifier, ConnectionLimits, InstanceInventory, InstanceProjectVersions,
    PostgresConfig, PostgresControlStore, PostgresCredentials, PostgresEndpoint,
};
use broker_presence::{
    BrokerOwnReadHttpConfig, BrokerPublishHttpConfig, LocalRealtimeHttpConfig, RealtimeHttpConfig,
    RealtimeServerRuntime, broker_instance_inventory_router, broker_realtime_router,
    local_realtime_router,
};
use broker_security::TokenKind;
use graphit_auth::{
    AdminSessionPolicy, CredentialSource, ExternalEffect, IssuerAuthorityPolicy, IssuerLimits,
    Permission, ProjectCapture, ProjectRecordAuthority, ProjectWriteOperation,
    RemoteServiceBindingInput, ScopeCapture, ScopeInput, ServiceAdmissionLimits,
    ServiceAuthorityInput, ServiceOrigin, ServicePurpose, ServiceRuleInput, SourceAuthorityPolicy,
    TtlPolicy,
    broker::{JsonLimits, VerifierLimits},
    profile::{
        AiService, BrokerService, LocalCredentials, OidcTokens, ProfileCredentials, ProfileData,
        ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
        Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeBindingRecord, ScopeLimits,
        StoreOwner,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationIdentityStore,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_inventory::GlobalProjectInventory,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::RuntimeKeyLimits,
    version_context::VersionContext,
};
use graphit_daemon::{
    auth_startup::{DaemonAuth, StartupPolicy},
    broker_verifiers::BrokerVerifierCache,
    now_publisher::{
        NowPublisherError, NowPublisherInput, NowPublisherLimits, start_now_publisher,
    },
    now_subscription::{NowRemoteCode, NowSubscriptionConfig, start_now_subscription},
    project_bootstrap::{ProjectBootstrapInput, ProjectBootstrapLimits, bootstrap_project},
    project_now::ProjectNowLimits,
};
use graphit_data::{
    knowledge_document_projection as h,
    knowledge_index::{
        KnowledgeHeadReadLimits, KnowledgeIndexContentLimits, KnowledgeIndexWriteLimits,
        publish_native_authored,
    },
};
use graphit_http::{
    client::{BoundedClient, ClientPolicy, Limits, NetworkRequest, NetworkRequestContext},
    websocket::{WebSocketClient, WebSocketLimits},
};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{
        CreateMemory, MemoryHeadReadLimits, MemoryStoreReadLimits, MemoryStoreWriteLimits,
        NativeMemoryStore,
    },
    reference_projection::AuthoredReferencesField,
    task_spec::CheckKind,
    task_store::{
        CreateCheck, CreateSession, CreateTask, NativeTaskStore, TaskStoreReadLimits,
        TaskStoreWriteLimits,
    },
};
use http::{HeaderMap, HeaderValue, Method, header};
use pg::n;
use secrecy::SecretString;
use serde_json::json;
use sqlx_core::{connection::Connection, executor::Executor};
use sqlx_postgres::PgConnection;
use std::{
    num::{NonZeroU16, NonZeroU32},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::net::TcpListener;
use zeroize::Zeroizing;

const LOCAL_RAW: &str = "gb_at_runtime-now-fixture";

fn seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn request(signal: &Cancellation, duration: Duration) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        signal,
        Instant::now() + duration,
    )
    .unwrap()
}
fn network() -> Limits {
    Limits {
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
    }
}
fn websocket(origin: &str) -> WebSocketClient {
    WebSocketClient::new(
        ClientPolicy {
            origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network(),
        WebSocketLimits {
            frame_bytes: n(65536),
            message_bytes: n(65536),
            send_bytes: n(65536),
        },
    )
    .unwrap()
}
fn cache() -> BrokerVerifierCache {
    BrokerVerifierCache::new(
        network(),
        VerifierLimits {
            token_bytes: 8192,
            json: JsonLimits {
                bytes: 8192,
                nodes: 256,
                depth: 16,
                scalar_bytes: 2048,
                collection_entries: 32,
            },
            max_keys: 8,
            metadata_ttl: Duration::from_secs(300),
            key_ttl: Duration::from_secs(300),
            unknown_key_cooldown: Duration::from_millis(100),
        },
        true,
        None,
        &[],
    )
}
enum Credential {
    Native(String),
    Local,
}
struct AuthFixture {
    auth: DaemonAuth,
    paths: InstallationPaths,
    project_paths: ProjectPaths,
    project: CapturedPublicationScope,
    project_record: ScopeBindingRecord,
    version: VersionContext,
    store: AuthStore,
    _directory: tempfile::TempDir,
}
impl AuthFixture {
    fn new(
        endpoint: &str,
        credential: Credential,
        project_root: Option<&std::path::Path>,
        permissions: &[Permission],
        original: &RequestContext<'_>,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let root = project_root.map_or_else(|| directory.path().join("project"), ToOwned::to_owned);
        std::fs::create_dir_all(&root).unwrap();
        let project_paths = ProjectPaths::at(&root, &paths).unwrap();
        let identity = ProjectIdentityStore::at(&project_paths)
            .writer(Duration::from_secs(5), original.cancellation())
            .unwrap()
            .ensure(&ProjectMetadata::new("actual-now-publisher").unwrap())
            .unwrap();
        let native = matches!(&credential, Credential::Native(_));
        let scope = ScopeLimits::new(16, 4096, 128).unwrap();
        let document = AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap();
        let profiles = ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 4,
            profiles: 4,
        };
        let rule = |purpose| ServiceRuleInput::Remote {
            origin: ServiceOrigin::Runtime,
            binding: RemoteServiceBindingInput::Broker {
                provider: "now",
                purpose,
                endpoint,
                resource: None,
            },
            credential: if native {
                CredentialSource::NativeAccess
            } else {
                CredentialSource::LocalBrokerKey
            },
            incoming: None,
        };
        let rules = [
            rule(ServicePurpose::PresencePublish),
            rule(ServicePurpose::PresenceRead),
        ];

        let source = || SourceAuthorityPolicy {
            ordinary: permissions,
            administrative: &[],
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
            },
        };
        let policy = StartupPolicy {
            bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
            runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
            scope,
            document,
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
                    rules: &rules,
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
        };
        let auth = DaemonAuth::start(
            &paths,
            std::slice::from_ref(&project_paths),
            policy,
            original,
        )
        .unwrap();
        let inventory = GlobalProjectInventory::at(&paths, 65536).unwrap();
        let bootstrap_limits = BootstrapLimits::new(65536, 256).unwrap();
        let mut selected = None;
        for name in ["branch/ação", "main"] {
            let private =
                installation_bootstrap::prepare(&paths, bootstrap_limits, original.snapshot())
                    .into_installation()
                    .unwrap();
            let bootstrapped = bootstrap_project(
                ProjectBootstrapInput {
                    project: &project_paths,
                    explicit_version: name,
                    pin: None,
                    detached_base: "",
                },
                ProjectBootstrapLimits {
                    scope,
                    native: native_limits(),
                },
                &inventory,
                private,
                original.snapshot(),
            )
            .unwrap();
            selected = Some(bootstrapped.version().clone());
        }
        let version = selected.unwrap();
        let project = CapturedPublicationScope::project(
            &project_paths,
            identity.id(),
            scope,
            original.snapshot(),
        )
        .unwrap();
        let project_record = project.binding().unwrap();
        let id = InstallationIdentityStore::new(&paths)
            .unwrap()
            .load()
            .unwrap()
            .unit_id()
            .unwrap()
            .clone();
        let captured = CapturedPublicationScope::installation(
            &paths,
            id,
            auth.installation()
                .live
                .allowed_roots()
                .map(ToOwned::to_owned),
            scope,
            original.snapshot(),
        )
        .unwrap();
        let store = AuthStore::new(
            AuthDocumentStore::capture(captured, document, original.snapshot()).unwrap(),
            profiles,
            original,
        )
        .unwrap();
        let before = store.load(original).unwrap();
        let result = store
            .mutate(before, original, |doc, ctx| {
                let service = BrokerService {
                    endpoint: endpoint.into(),
                    mcp_resource: None,
                    allow_anonymous: false,
                };
                doc.add_provider(
                    "now".into(),
                    ProviderConfig {
                        kind: if native {
                            ProviderKind::Broker(service)
                        } else {
                            ProviderKind::Local {
                                allow_aws_credential_chain: false,
                                broker: Some(service),
                            }
                        },
                        embedding: AiService::Broker,
                        rerank: AiService::Broker,
                        s3: S3Location {
                            bucket: None,
                            region: None,
                            endpoint: None,
                            prefix: None,
                            source: if native {
                                S3Source::BrokerScope
                            } else {
                                S3Source::LocalCredentials
                            },
                        },
                    },
                    doc.revision(),
                    1,
                    ctx,
                )?;
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("now").unwrap().revision(),
                    profile_revision: None,
                };
                let credentials = match &credential {
                    Credential::Native(raw) => ProfileCredentials::Broker {
                        issuer: endpoint.into(),
                        subject: "fixture-sub".into(),
                        s3_disabled: true,
                        tokens: OidcTokens {
                            access_token: Secret::new(SecretString::from(raw.clone())),
                            refresh_token: None,
                            id_token: Secret::new(SecretString::from("historical-fixture")),
                            expires_at: i64::try_from(
                                Duration::from_secs(seconds() + 3600).as_micros(),
                            )
                            .unwrap(),
                        },
                    },
                    Credential::Local => ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new(SecretString::from("runtime-fixture"))),
                        broker_key: Some(Secret::new(SecretString::from(LOCAL_RAW))),
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: None,
                    }),
                };
                doc.replace_profile_data(
                    ProfileData {
                        name: "now".into(),
                        provider: "now".into(),
                        username: "fixture".into(),
                        organization: None,
                        teams: vec![],
                        credentials,
                    },
                    expected,
                    1,
                    ctx,
                )?;
                doc.activate("now", doc.revision(), ctx)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(result.effect(), AuthPublicationEffect::Applied);
        assert!(result.failure().is_none());
        Self {
            auth,
            paths,
            project_paths,
            project,
            project_record,
            version,
            store,
            _directory: directory,
        }
    }
    fn runtime_key(&self, original: &RequestContext<'_>) -> Zeroizing<String> {
        let key = self.auth.runtime().check(original.snapshot()).unwrap();
        Zeroizing::new(
            String::from_utf8(key.secret(original.snapshot()).unwrap().to_vec()).unwrap(),
        )
    }
}

struct Server {
    origin: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl Server {
    fn start(listener: TcpListener, app: axum::Router) -> Self {
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (stop, rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });
        Self {
            origin,
            stop: Some(stop),
            task: Some(task),
        }
    }
    async fn shutdown(mut self) {
        let _ = self.stop.take().unwrap().send(());
        tokio::time::timeout(Duration::from_secs(3), self.task.as_mut().unwrap())
            .await
            .unwrap()
            .unwrap();
        self.task.take();
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn pg_config(f: &pg::Fixture) -> PostgresConfig {
    PostgresConfig::new(
        PostgresEndpoint {
            host: "127.0.0.1".into(),
            port: NonZeroU16::new(f.port).unwrap(),
            database: "postgres".into(),
        },
        PostgresCredentials::new(
            "runtime_now_owner".into(),
            Zeroizing::new("ephemeral-owner".into()),
        ),
        f.ca.clone(),
        ConnectionLimits {
            text_bytes: n(256),
            ca_bytes: n(8192),
        },
        pg::pool(2),
        pg::limits(),
    )
    .unwrap()
}
fn realtime() -> RealtimeHttpConfig {
    RealtimeHttpConfig {
        required_scopes: vec!["presence-sdk".into()],
        grants_json_bytes: n(65536),
        inventory_json_bytes: n(65536),
        command_bytes: n(65536),
        outbound_bytes: n(65536),
        operation_timeout: Duration::from_secs(2),
    }
}
async fn acl(
    connection: &mut PgConnection,
    caps: &[&str],
    projects: &[graphit_core::project_identity::ProjectId],
) {
    connection.execute("BEGIN; DELETE FROM public.resource_grants; UPDATE public.resource_acl_state SET revision=revision+1,updated_at=0 WHERE id=1").await.unwrap();
    sqlx_core::query::query("INSERT INTO public.resource_grants(id,name,access_kind,principal,s3_route,created_at,updated_at) VALUES('runtime-now','fixture','authenticated','','',0,0)").execute(&mut *connection).await.unwrap();
    for cap in caps {
        sqlx_core::query::query(
            "INSERT INTO public.grant_capabilities(grant_id,value) VALUES('runtime-now',$1)",
        )
        .bind(*cap)
        .execute(&mut *connection)
        .await
        .unwrap();
    }
    for project in projects {
        sqlx_core::query::query(
            "INSERT INTO public.grant_projects(grant_id,value) VALUES('runtime-now',$1)",
        )
        .bind(project.to_string())
        .execute(&mut *connection)
        .await
        .unwrap();
    }
    connection.execute("COMMIT").await.unwrap();
}

fn native_limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(1048576, 65536, 256, 8 * 1024 * 1024).unwrap()
}
fn heads() -> ProjectNowLimits {
    ProjectNowLimits {
        task: TaskStoreReadLimits {
            native: native_limits(),
            batch_rows: NonZeroU32::new(1).unwrap(),
        },
        memory: MemoryHeadReadLimits {
            batch_rows: NonZeroU32::new(1).unwrap(),
            native: native_limits(),
        },
        knowledge: KnowledgeHeadReadLimits {
            max_slug_bytes: n(256),
            batch_rows: NonZeroU32::new(1).unwrap(),
            native: native_limits(),
        },
    }
}
fn wire() -> NowPublisherLimits {
    NowPublisherLimits {
        command_bytes: n(65536),
        control_bytes: n(65536),
    }
}
fn ordinary() -> Vec<Permission> {
    vec![
        Permission::External(ExternalEffect::RemoteRequest),
        Permission::Read(Module::Task),
        Permission::Read(Module::Memory),
        Permission::Read(Module::Knowledge),
        Permission::WriteTask,
        Permission::WriteOwnTask,
        Permission::CoordinateTask,
        Permission::WriteMemory,
        Permission::MutateModule(Module::Knowledge),
    ]
}
impl AuthFixture {
    fn project_id(&self) -> graphit_core::project_identity::ProjectId {
        let StoreOwner::Project { project_id } = self.project.owner() else {
            panic!()
        };
        *project_id
    }
    fn instance_id(&self) -> &str {
        let StoreOwner::Installation { installation_id } = self.auth.installation().live.owner()
        else {
            panic!()
        };
        installation_id.as_str()
    }
    fn scope<'a>(&'a self, version: &'a VersionContext) -> ScopeInput<'a> {
        ScopeInput {
            installation: self.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.project,
                    record: &self.project_record,
                },
                version,
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        }
    }
}
fn artifacts() -> ArtifactLimits {
    ArtifactLimits::new(
        1048576,
        65536,
        CopyLimits::new(1048576, 128, 65536, 16).unwrap(),
    )
    .unwrap()
}
fn task_write() -> TaskStoreWriteLimits {
    TaskStoreWriteLimits {
        read: heads().task,
        artifacts: artifacts(),
        references: graphit_records::task_store::TaskReferencesInputLimits {
            max_entries: n(65536),
            max_text_bytes: n(1048576),
        },
    }
}
fn memory_write() -> MemoryStoreWriteLimits {
    MemoryStoreWriteLimits {
        read: MemoryStoreReadLimits {
            max_query_bytes: n(4 * 1024 * 1024),
            batch_rows: NonZeroU32::new(1).unwrap(),
            native: native_limits(),
        },
        artifacts: artifacts(),
        max_input_bytes: n(1048576),
        max_row_bytes: n(2 * 1048576),
    }
}
fn knowledge_write() -> KnowledgeIndexWriteLimits {
    KnowledgeIndexWriteLimits {
        content: KnowledgeIndexContentLimits {
            max_identifier_bytes: n(256),
        },
        native: native_limits(),
        artifacts: artifacts(),
    }
}
async fn seed<'a>(
    auth: &'a DaemonAuth,
    caller: &'a graphit_auth::AuthenticatedCaller<'a>,
    original: &'a RequestContext<'a>,
    scope: ScopeInput<'a>,
    name: &str,
) {
    let authority = ProjectRecordAuthority::new(auth.issuer());
    let task = authority
        .admit_authenticated_write(caller, original, scope, ProjectWriteOperation::Task)
        .unwrap();
    let session = NativeTaskStore::create_session(
        original.snapshot(),
        &task,
        CreateSession {
            idempotency_key: format!("{name}-session"),
            title: name.into(),
            description: "PRIVATE_SESSION_BODY".into(),
            strategy: "actual native".into(),
        },
        AuthoredReferencesField::Omitted,
        task_write(),
    )
    .await
    .unwrap();
    let saved = NativeTaskStore::create_task(
        original.snapshot(),
        &task,
        CreateTask {
            commit: None,
            idempotency_key: format!("{name}-task"),
            session_id: session.id,
            parent_id: None,
            depends_on: vec![],
            title: name.into(),
            description: "PRIVATE_TASK_BODY".into(),
            task_type: "task".into(),
            priority: 2,
            schedule: None,
            checks: vec![
                CreateCheck {
                    kind: CheckKind::Acceptance,
                    text: "Publish actual fixture head".into(),
                },
                CreateCheck {
                    kind: CheckKind::Test,
                    text: "Observe the complete fixture head".into(),
                },
            ],
        },
        AuthoredReferencesField::Omitted,
        task_write(),
    )
    .await
    .unwrap();
    NativeTaskStore::claim_task(
        original.snapshot(),
        &task,
        saved.specification.id,
        Duration::from_secs(120),
        task_write(),
    )
    .await
    .unwrap();
    let coordinate = authority
        .admit_authenticated_write(
            caller,
            original,
            scope,
            ProjectWriteOperation::CoordinateTask,
        )
        .unwrap();
    NativeTaskStore::claim_session(
        original.snapshot(),
        &coordinate,
        session.id,
        Duration::from_secs(120),
        task_write(),
    )
    .await
    .unwrap();
    add_memory(auth, caller, original, scope, name).await;
    let knowledge = authority
        .admit_authenticated_write(caller, original, scope, ProjectWriteOperation::Knowledge)
        .unwrap();
    let page = h::AuthoredPageFields {
        slug: Some(name),
        title: Some(name),
        body: Some("PRIVATE_KNOWLEDGE_BODY"),
        kind: Some("page"),
        summary: Some("Knowledge fixture summary"),
        description: None,
        summary_truncated: false,
        source_path: "private/wiki.md",
        physical_digest: None,
        compilation_digest: h::CompilationDigest32::from_bytes([1; 32]),
        recipe: h::RecipeFields {
            id: h::AUTHORED_RECIPE_ID,
            parameters: Some(h::AuthoredParameters {
                fallback_title: "fallback",
                kind_on_absent: "page",
                max_summary_bytes: n(256),
            }),
        },
        updated_at: Some(UNIX_EPOCH + Duration::from_nanos(9_007_199_254_740_993)),
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: &[],
    };
    let pages = [page];
    let prepared = h::prepare(
        original.snapshot(),
        &pages,
        &[],
        h::KnowledgeDocumentProjectionLimits {
            max_documents: n(128),
            max_observations: n(128),
            max_rows: n(1024),
            max_field_bytes: n(65536),
            max_text_bytes: n(4 * 1024 * 1024),
        },
    )
    .unwrap();
    publish_native_authored(
        original.snapshot(),
        &knowledge,
        &prepared,
        knowledge_write(),
    )
    .await
    .unwrap();
}
async fn add_memory<'a>(
    auth: &'a DaemonAuth,
    caller: &'a graphit_auth::AuthenticatedCaller<'a>,
    original: &'a RequestContext<'a>,
    scope: ScopeInput<'a>,
    name: &str,
) {
    let authority = ProjectRecordAuthority::new(auth.issuer());
    let memory = authority
        .admit_authenticated_write(caller, original, scope, ProjectWriteOperation::Memory)
        .unwrap();
    NativeMemoryStore::create(
        original.snapshot(),
        &memory,
        CreateMemory {
            idempotency_key: format!("{name}-memory"),
            title: name.into(),
            body: "PRIVATE_MEMORY_BODY".into(),
            attributes: MemoryAttributes {
                classification: MemoryClass::Decision,
                mandatory: false,
                important: true,
            },
            tags: vec![],
            references: None,
        },
        memory_write(),
    )
    .await
    .unwrap();
}
async fn register_native(
    http: &BoundedClient,
    origin: &str,
    raw: &str,
    auth: &AuthFixture,
    original: &RequestContext<'_>,
    versions: &[&str],
) {
    let mut authorization = HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
    authorization.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, authorization);
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let body =
        json!({"projects":[{"project_id":auth.project_id(),"versions":versions}]}).to_string();
    let response = http
        .execute(
            NetworkRequest {
                method: Method::PUT,
                url: &format!("{origin}/v1/instances/{}/inventory", auth.instance_id()),
                headers: &headers,
                body: body.as_bytes(),
            },
            NetworkRequestContext::with_deadline(original.deadline(), original.cancellation()),
        )
        .await
        .unwrap();
    assert!(response.status().is_success());
}

async fn seed_empty<'a>(
    auth: &'a DaemonAuth,
    caller: &'a graphit_auth::AuthenticatedCaller<'a>,
    original: &'a RequestContext<'a>,
    scope: ScopeInput<'a>,
) {
    let authority = ProjectRecordAuthority::new(auth.issuer());
    let task = authority
        .admit_authenticated_write(caller, original, scope, ProjectWriteOperation::Task)
        .unwrap();
    NativeTaskStore::create_session(
        original.snapshot(),
        &task,
        CreateSession {
            idempotency_key: "open-empty-session".into(),
            title: "empty".into(),
            description: "actual empty native".into(),
            strategy: "EOF".into(),
        },
        AuthoredReferencesField::Omitted,
        task_write(),
    )
    .await
    .unwrap();
    let memory = authority
        .admit_authenticated_write(caller, original, scope, ProjectWriteOperation::Memory)
        .unwrap();
    let saved = NativeMemoryStore::create(
        original.snapshot(),
        &memory,
        CreateMemory {
            idempotency_key: "empty-memory".into(),
            title: "empty".into(),
            body: "PRIVATE_EMPTY_BODY".into(),
            attributes: MemoryAttributes {
                classification: MemoryClass::Decision,
                mandatory: false,
                important: false,
            },
            tags: vec![],
            references: None,
        },
        memory_write(),
    )
    .await
    .unwrap();
    NativeMemoryStore::delete(
        original.snapshot(),
        &memory,
        saved.id,
        graphit_records::memory_store::DeleteMemory {
            expected_revision: saved.revision,
            idempotency_key: "tombstone-empty-memory".into(),
            reason: "native EOF test".into(),
        },
        memory_write(),
    )
    .await
    .unwrap();
    let knowledge = authority
        .admit_authenticated_write(caller, original, scope, ProjectWriteOperation::Knowledge)
        .unwrap();
    let prepared = h::prepare(
        original.snapshot(),
        &[],
        &[],
        h::KnowledgeDocumentProjectionLimits {
            max_documents: n(128),
            max_observations: n(128),
            max_rows: n(1024),
            max_field_bytes: n(65536),
            max_text_bytes: n(4 * 1024 * 1024),
        },
    )
    .unwrap();
    publish_native_authored(
        original.snapshot(),
        &knowledge,
        &prepared,
        knowledge_write(),
    )
    .await
    .unwrap();
}

async fn channels_and_lifecycle(
    auth: &AuthFixture,
    local_auth: &AuthFixture,
    original: &RequestContext<'_>,
    endpoints: (&str, &str, &str),
    connection: &mut sqlx_postgres::PgConnection,
    project_ids: &[graphit_core::project_identity::ProjectId],
    capabilities: &[&str],
) {
    let (origin, local_origin, raw) = endpoints;
    let transport = websocket(origin);
    let http = BoundedClient::new(
        ClientPolicy {
            origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network(),
    )
    .unwrap();
    let verifier_cache = cache();
    let local_transport = websocket(local_origin);
    let native_key = auth.runtime_key(original);
    let caller = auth
        .auth
        .issuer()
        .authenticate_runtime(&native_key, original)
        .unwrap();
    let local_key = local_auth.runtime_key(original);
    let local_caller = local_auth
        .auth
        .issuer()
        .authenticate_runtime(&local_key, original)
        .unwrap();
    let authority = ProjectRecordAuthority::new(auth.auth.issuer());
    let local_authority = ProjectRecordAuthority::new(local_auth.auth.issuer());
    let native_scope = auth.scope(&auth.version);
    let branch = VersionContext::select(
        &local_auth.project_paths,
        graphit_core::version_context::Checkout::NonGit,
        "branch/ação",
        None,
        "",
    )
    .unwrap();
    let local_scope = local_auth.scope(&branch);
    seed(&auth.auth, &caller, original, native_scope, "first").await;
    let mut selection = auth
        .auth
        .issuer()
        .capture_presence_publish_service(&caller, "now")
        .unwrap();
    let verifier = verifier_cache.get(&mut selection).unwrap();
    let lease = selection.native_credential(&verifier).await.unwrap();
    let mut local_selection = local_auth
        .auth
        .issuer()
        .capture_presence_publish_service(&local_caller, "now")
        .unwrap();
    let local_lease = local_selection.credential().unwrap();
    let input = NowPublisherInput {
        authority: &authority,
        caller: &caller,
        scope: native_scope,
        heads: heads(),
    };
    let local_input = NowPublisherInput {
        authority: &local_authority,
        caller: &local_caller,
        scope: local_scope,
        heads: heads(),
    };
    let foreign = request(original.cancellation(), Duration::from_secs(180));
    assert!(matches!(
        start_now_publisher(&transport, &lease, &foreign, input, wire()).await,
        Err(NowPublisherError::DifferentOriginal)
    ));
    let mut small = wire();
    small.command_bytes = n(32);
    assert!(matches!(
        start_now_publisher(&transport, &lease, original, input, small).await,
        Err(NowPublisherError::Limit)
    ));
    let mut p1 = start_now_publisher(&transport, &lease, original, input, wire())
        .await
        .unwrap();
    let mut p2 = start_now_publisher(
        &local_transport,
        &local_lease,
        original,
        local_input,
        wire(),
    )
    .await
    .unwrap();
    let mut read_selection = auth
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "now")
        .unwrap();
    let read_verifier = verifier_cache.get(&mut read_selection).unwrap();
    let read_lease = read_selection
        .native_credential(&read_verifier)
        .await
        .unwrap();
    let observer = NowSubscriptionConfig {
        project_id: auth.project_id(),
        instance_id: None,
        version: None,
        command_bytes: n(65536),
        snapshot_bytes: n(65536),
    };
    let mut subscriber = start_now_subscription(&transport, &read_lease, original, observer)
        .await
        .unwrap();
    let snapshot = subscriber.next().await.unwrap();
    assert_eq!(snapshot.instances().len(), 2);
    let mut expected = vec![
        (auth.instance_id(), "main"),
        (local_auth.instance_id(), "branch/ação"),
    ];
    expected.sort();
    assert_eq!(
        snapshot
            .instances()
            .iter()
            .map(|i| (i.instance_id(), i.version()))
            .collect::<Vec<_>>(),
        expected
    );
    let full = snapshot
        .instances()
        .iter()
        .find(|i| i.instance_id() == auth.instance_id())
        .unwrap()
        .now();
    let data = serde_json::to_value(full).unwrap();
    assert_eq!(data["sections"]["tasks"]["total"], 1);
    assert_eq!(data["sections"]["sessions"]["total"], 1);
    assert_eq!(data["sections"]["memories"]["total"], 1);
    assert_eq!(
        data["sections"]["knowledge"]["items"][0]["updated_at"],
        "9007199254740993"
    );
    let empty = snapshot
        .instances()
        .iter()
        .find(|i| i.instance_id() == local_auth.instance_id())
        .unwrap()
        .now();
    let absent = serde_json::to_value(empty).unwrap();
    // A missing named ref reads the default corpus through the real SDK inheritance.
    for section in ["tasks", "sessions", "memories", "knowledge"] {
        assert_eq!(absent["sections"][section]["availability"], "available");
        assert_eq!(
            absent["sections"][section]["total"],
            data["sections"][section]["total"]
        );
    }
    assert!(!serde_json::to_string(&data).unwrap().contains("PRIVATE_"));
    assert!(full.generated_at().parse::<u128>().unwrap() > 9_007_199_254_740_991);
    add_memory(&auth.auth, &caller, original, native_scope, "second").await;
    // A heartbeat may repeat the prior genuine observation while a real SDK
    // read is pending. Keep driving both owners until the new head is observed.
    loop {
        let (one, two, replacement) = tokio::join!(p1.next(), p2.next(), subscriber.next());
        one.unwrap();
        two.unwrap();
        let replacement = replacement.unwrap();
        let first = replacement
            .instances()
            .iter()
            .find(|i| i.instance_id() == auth.instance_id())
            .unwrap();
        if serde_json::to_value(first.now()).unwrap()["sections"]["memories"]["total"] == 2 {
            break;
        }
    }
    assert!(matches!(
        start_now_publisher(&transport, &read_lease, original, input, wire()).await,
        Err(NowPublisherError::WrongPurpose)
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), p2.next())
            .await
            .is_err()
    );
    assert!(matches!(
        p2.next().await,
        Err(NowPublisherError::Closed(1000))
    ));
    p2.shutdown().await.unwrap();
    loop {
        // p1 remains a live owner while observing p2's departure. Every next
        // future finishes; abandoning one would itself terminate that channel.
        let (publication, snapshot) = tokio::join!(p1.next(), subscriber.next());
        publication.unwrap();
        if snapshot.unwrap().instances().len() == 1 {
            break;
        }
    }
    register_native(&http, origin, raw, auth, original, &["branch/ação"]).await;
    let terminal = loop {
        if let Err(error) = p1.next().await {
            break error;
        }
    };
    assert!(
        matches!(
            terminal,
            NowPublisherError::Remote(NowRemoteCode::NotRegistered)
                | NowPublisherError::Closed(1008)
        ),
        "{terminal:?}"
    );
    assert!(matches!(
        p1.next().await,
        Err(NowPublisherError::Closed(1000))
    ));
    p1.shutdown().await.unwrap();
    subscriber.shutdown().await.unwrap();
    register_native(&http, origin, raw, auth, original, &["main", "branch/ação"]).await;
    let mut restored = start_now_publisher(&transport, &lease, original, input, wire())
        .await
        .unwrap();
    let mut replacement = start_now_publisher(&transport, &lease, original, input, wire())
        .await
        .unwrap();
    let superseded = loop {
        if let Err(error) = restored.next().await {
            break error;
        }
    };
    assert!(
        matches!(
            superseded,
            NowPublisherError::Remote(NowRemoteCode::Superseded) | NowPublisherError::Closed(1008)
        ),
        "{superseded:?}"
    );
    restored.shutdown().await.unwrap();
    acl(
        connection,
        &[
            "presence:read",
            "task:read",
            "memory:read",
            "knowledge:read",
        ],
        project_ids,
    )
    .await;
    let denied = loop {
        if let Err(error) = replacement.next().await {
            break error;
        }
    };
    assert!(
        matches!(
            denied,
            NowPublisherError::Remote(NowRemoteCode::Forbidden) | NowPublisherError::Closed(1008)
        ),
        "{denied:?}"
    );
    replacement.shutdown().await.unwrap();
    acl(connection, capabilities, project_ids).await;
}

async fn corpus_section_matrix(
    origin: &str,
    raw: &str,
    original: &RequestContext<'_>,
    connection: &mut sqlx_postgres::PgConnection,
    project_ids: &mut Vec<graphit_core::project_identity::ProjectId>,
    capabilities: &[&str],
) {
    let transport = websocket(origin);
    let http = BoundedClient::new(
        ClientPolicy {
            origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network(),
    )
    .unwrap();
    let verifier_cache = cache();
    // Corpora denials differ from PresencePublish denial; empty EOF and inherited data differ from physical absence.
    for mode in 0..5 {
        let permissions = match mode {
            1 => vec![
                Permission::External(ExternalEffect::RemoteRequest),
                Permission::Read(Module::Task),
            ],
            2 => vec![Permission::External(ExternalEffect::RemoteRequest)],
            _ => ordinary(),
        };
        let matrix_auth = AuthFixture::new(
            origin,
            Credential::Native(raw.to_string()),
            None,
            &permissions,
            original,
        );
        project_ids.push(matrix_auth.project_id());
        acl(connection, capabilities, project_ids).await;
        register_native(&http, origin, raw, &matrix_auth, original, &["main"]).await;
        let matrix_key = matrix_auth.runtime_key(original);
        let matrix_caller = matrix_auth
            .auth
            .issuer()
            .authenticate_runtime(&matrix_key, original)
            .unwrap();
        let matrix_authority = ProjectRecordAuthority::new(matrix_auth.auth.issuer());
        let matrix_scope = matrix_auth.scope(&matrix_auth.version);
        if mode == 0 {
            seed_empty(&matrix_auth.auth, &matrix_caller, original, matrix_scope).await;
        }
        if mode == 3 {
            std::fs::create_dir_all(matrix_auth.project.runtime_root()).unwrap();
            drop(
                graphit_fs::create_private_directory(
                    &matrix_auth
                        .project_paths
                        .project_runtime(matrix_auth.project_id(), &["memory"])
                        .unwrap(),
                )
                .unwrap(),
            );
        }
        let mut matrix_selection = matrix_auth
            .auth
            .issuer()
            .capture_presence_publish_service(&matrix_caller, "now")
            .unwrap();
        let matrix_verifier = verifier_cache.get(&mut matrix_selection).unwrap();
        let matrix_lease = matrix_selection
            .native_credential(&matrix_verifier)
            .await
            .unwrap();
        let matrix_input = NowPublisherInput {
            authority: &matrix_authority,
            caller: &matrix_caller,
            scope: matrix_scope,
            heads: heads(),
        };
        let matrix_publisher =
            start_now_publisher(&transport, &matrix_lease, original, matrix_input, wire())
                .await
                .unwrap();
        let mut matrix_read_selection = matrix_auth
            .auth
            .issuer()
            .capture_presence_read_service(&matrix_caller, "now")
            .unwrap();
        let matrix_read_verifier = verifier_cache.get(&mut matrix_read_selection).unwrap();
        let matrix_read_lease = matrix_read_selection
            .native_credential(&matrix_read_verifier)
            .await
            .unwrap();
        let matrix_observer = NowSubscriptionConfig {
            project_id: matrix_auth.project_id(),
            instance_id: None,
            version: None,
            command_bytes: n(65536),
            snapshot_bytes: n(65536),
        };
        let mut matrix_subscriber =
            start_now_subscription(&transport, &matrix_read_lease, original, matrix_observer)
                .await
                .unwrap();
        let observed = matrix_subscriber.next().await.unwrap();
        assert_eq!(observed.instances().len(), 1);
        let data = serde_json::to_value(observed.instances()[0].now()).unwrap();
        for section in ["tasks", "sessions", "memories", "knowledge"] {
            let failed = mode == 2
                || (mode == 1 && matches!(section, "memories" | "knowledge"))
                || (mode == 3 && section == "memories");
            if failed {
                assert_eq!(data["sections"][section]["availability"], "failed");
                assert!(
                    data["sections"][section]["items"]
                        .as_array()
                        .unwrap()
                        .is_empty()
                );
                assert!(data["sections"][section]["total"].is_null());
                assert!(data["sections"][section]["has_more"].is_null());
                assert_eq!(
                    data["sections"][section]["error"],
                    if mode == 3 {
                        "invalid_data"
                    } else {
                        "forbidden"
                    }
                );
            } else {
                assert_eq!(
                    data["sections"][section]["availability"],
                    if mode == 0 { "available" } else { "no_corpus" }
                );
                assert_eq!(data["sections"][section]["total"], 0);
                assert_eq!(data["sections"][section]["has_more"], false);
                assert!(data["sections"][section]["error"].is_null());
            }
        }
        matrix_publisher.shutdown().await.unwrap();
        loop {
            if matrix_subscriber
                .next()
                .await
                .unwrap()
                .instances()
                .is_empty()
            {
                break;
            }
        }
        matrix_subscriber.shutdown().await.unwrap();
        assert!(
            matrix_auth
                .auth
                .remove_runtime(original.snapshot())
                .failure()
                .is_none()
        );
    }
}

async fn registered_owner_conflict(
    origin: &str,
    raw: &str,
    key: &Ed25519KeyPair,
    original: &RequestContext<'_>,
    connection: &mut sqlx_postgres::PgConnection,
    project_ids: &mut Vec<graphit_core::project_identity::ProjectId>,
    capabilities: &[&str],
) {
    let transport = websocket(origin);
    let http = BoundedClient::new(
        ClientPolicy {
            origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network(),
    )
    .unwrap();
    let verifier_cache = cache();
    // A different actual verified subject owns this inventory; our caller cannot replace it.
    let conflict_auth = AuthFixture::new(
        origin,
        Credential::Native(raw.to_string()),
        None,
        &ordinary(),
        original,
    );
    project_ids.push(conflict_auth.project_id());
    acl(connection, capabilities, project_ids).await;
    let owner_claims = json!({"iss":origin,"sub":"another-verified-owner","aud":["broker-api"],"client_id":"graphit-cli",
        "jti":"owner-conflict-test","token_use":"access","scope":"presence-sdk","iat":seconds()-1,"exp":seconds()+3600});
    let owner_message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(json!({"alg":"EdDSA","kid":"actual","typ":"JWT"}).to_string()),
        URL_SAFE_NO_PAD.encode(owner_claims.to_string())
    );
    let owner_raw = Zeroizing::new(format!(
        "{owner_message}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(owner_message.as_bytes()).as_ref())
    ));
    register_native(
        &http,
        origin,
        &owner_raw,
        &conflict_auth,
        original,
        &["main"],
    )
    .await;
    let conflict_key = conflict_auth.runtime_key(original);
    let conflict_caller = conflict_auth
        .auth
        .issuer()
        .authenticate_runtime(&conflict_key, original)
        .unwrap();
    let conflict_authority = ProjectRecordAuthority::new(conflict_auth.auth.issuer());
    let mut conflict_selection = conflict_auth
        .auth
        .issuer()
        .capture_presence_publish_service(&conflict_caller, "now")
        .unwrap();
    let conflict_verifier = verifier_cache.get(&mut conflict_selection).unwrap();
    let conflict_lease = conflict_selection
        .native_credential(&conflict_verifier)
        .await
        .unwrap();
    let conflict_input = NowPublisherInput {
        authority: &conflict_authority,
        caller: &conflict_caller,
        scope: conflict_auth.scope(&conflict_auth.version),
        heads: heads(),
    };
    assert!(matches!(
        start_now_publisher(
            &transport,
            &conflict_lease,
            original,
            conflict_input,
            wire()
        )
        .await,
        Err(NowPublisherError::Remote(NowRemoteCode::OwnerConflict))
    ));
    assert!(
        conflict_auth
            .auth
            .remove_runtime(original.snapshot())
            .failure()
            .is_none()
    );
}

async fn ordinary_and_service_lifetimes(auth: &AuthFixture, origin: &str) {
    let transport = websocket(origin);
    let verifier_cache = cache();
    let authority = ProjectRecordAuthority::new(auth.auth.issuer());
    let native_scope = auth.scope(&auth.version);
    // A finite ordinary operation still expires without replacing its Original.
    let finite_signal = Cancellation::default();
    let finite = request(&finite_signal, Duration::from_secs(2));
    let finite_key = auth.runtime_key(&finite);
    let finite_caller = auth
        .auth
        .issuer()
        .authenticate_runtime(&finite_key, &finite)
        .unwrap();
    let mut finite_selection = auth
        .auth
        .issuer()
        .capture_presence_publish_service(&finite_caller, "now")
        .unwrap();
    let finite_verifier = verifier_cache.get(&mut finite_selection).unwrap();
    let finite_lease = finite_selection
        .native_credential(&finite_verifier)
        .await
        .unwrap();
    let finite_input = NowPublisherInput {
        authority: &authority,
        caller: &finite_caller,
        scope: native_scope,
        heads: heads(),
    };
    let mut finite_publisher =
        start_now_publisher(&transport, &finite_lease, &finite, finite_input, wire())
            .await
            .unwrap();
    assert!(matches!(
        finite_publisher.next().await,
        Err(NowPublisherError::Request(_))
    ));
    assert!(matches!(
        finite_publisher.next().await,
        Err(NowPublisherError::Closed(1000))
    ));
    finite_publisher.shutdown().await.unwrap();

    // Service None is a real cancelable Original, not a deadline sentinel.
    let service_signal = Cancellation::default();
    let service = RequestContext::service(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        &service_signal,
    )
    .unwrap();
    assert!(service.deadline().is_none());
    let service_key = auth.runtime_key(&service);
    let service_caller = auth
        .auth
        .issuer()
        .authenticate_runtime(&service_key, &service)
        .unwrap();
    let mut service_selection = auth
        .auth
        .issuer()
        .capture_presence_publish_service(&service_caller, "now")
        .unwrap();
    let service_verifier = verifier_cache.get(&mut service_selection).unwrap();
    let service_lease = service_selection
        .native_credential(&service_verifier)
        .await
        .unwrap();
    let service_input = NowPublisherInput {
        authority: &authority,
        caller: &service_caller,
        scope: native_scope,
        heads: heads(),
    };
    let mut service_publisher =
        start_now_publisher(&transport, &service_lease, &service, service_input, wire())
            .await
            .unwrap();
    service_publisher.next().await.unwrap();
    service_signal.cancel();
    assert!(matches!(
        service_publisher.next().await,
        Err(NowPublisherError::Request(_))
    ));
    assert!(matches!(
        service_publisher.next().await,
        Err(NowPublisherError::Closed(1000))
    ));
    service_publisher.shutdown().await.unwrap();
}

async fn signed_expiry_without_original_deadline(
    origin: &str,
    key: &Ed25519KeyPair,
    original: &RequestContext<'_>,
    connection: &mut sqlx_postgres::PgConnection,
    project_ids: &mut Vec<graphit_core::project_identity::ProjectId>,
    capabilities: &[&str],
) {
    let transport = websocket(origin);
    let http = BoundedClient::new(
        ClientPolicy {
            origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network(),
    )
    .unwrap();
    let verifier_cache = cache();
    // JWT expiry is independent of the absence of an Original clock deadline.
    let expiry_signal = Cancellation::default();
    let expiry = RequestContext::service(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        &expiry_signal,
    )
    .unwrap();
    let short_claims = json!({"iss":origin,"sub":"fixture-sub","aud":["broker-api"],"client_id":"graphit-cli",
        "jti":"actual-short-native","token_use":"access","scope":"presence-sdk","iat":seconds()-1,"exp":seconds()+5});
    let short_message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(json!({"alg":"EdDSA","kid":"actual","typ":"JWT"}).to_string()),
        URL_SAFE_NO_PAD.encode(short_claims.to_string())
    );
    let short_raw = Zeroizing::new(format!(
        "{short_message}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(short_message.as_bytes()).as_ref())
    ));
    let short_auth = AuthFixture::new(
        origin,
        Credential::Native(short_raw.to_string()),
        None,
        &ordinary(),
        original,
    );
    project_ids.push(short_auth.project_id());
    acl(connection, capabilities, project_ids).await;
    register_native(&http, origin, &short_raw, &short_auth, &expiry, &["main"]).await;
    let short_key = short_auth.runtime_key(&expiry);
    let short_caller = short_auth
        .auth
        .issuer()
        .authenticate_runtime(&short_key, &expiry)
        .unwrap();
    let short_authority = ProjectRecordAuthority::new(short_auth.auth.issuer());
    let mut short_selection = short_auth
        .auth
        .issuer()
        .capture_presence_publish_service(&short_caller, "now")
        .unwrap();
    let short_verifier = verifier_cache.get(&mut short_selection).unwrap();
    let short_lease = short_selection
        .native_credential(&short_verifier)
        .await
        .unwrap();
    let short_input = NowPublisherInput {
        authority: &short_authority,
        caller: &short_caller,
        scope: short_auth.scope(&short_auth.version),
        heads: heads(),
    };
    let mut short_publisher =
        start_now_publisher(&transport, &short_lease, &expiry, short_input, wire())
            .await
            .unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match short_publisher.next().await {
                Ok(()) => continue,
                Err(
                    NowPublisherError::Admission(_)
                    | NowPublisherError::Remote(NowRemoteCode::Unauthenticated)
                    | NowPublisherError::Closed(1008),
                ) => break,
                Err(error) => panic!("actual signed expiry expected: {error:?}"),
            }
        }
    })
    .await
    .expect("fixture watchdog: signed JWT expiry did not terminate the publisher");
    assert!(expiry.deadline().is_none());
    assert!(matches!(
        short_publisher.next().await,
        Err(NowPublisherError::Closed(1000))
    ));
    short_publisher.shutdown().await.unwrap();
    assert!(
        short_auth
            .auth
            .remove_runtime(expiry.snapshot())
            .failure()
            .is_none()
    );
}

async fn original_cancellation(
    auth: &AuthFixture,
    origin: &str,
    original: &RequestContext<'_>,
    signal: &Cancellation,
) {
    let transport = websocket(origin);
    let verifier_cache = cache();
    let native_key = auth.runtime_key(original);
    let caller = auth
        .auth
        .issuer()
        .authenticate_runtime(&native_key, original)
        .unwrap();
    let authority = ProjectRecordAuthority::new(auth.auth.issuer());
    let native_scope = auth.scope(&auth.version);
    let mut selection = auth
        .auth
        .issuer()
        .capture_presence_publish_service(&caller, "now")
        .unwrap();
    let verifier = verifier_cache.get(&mut selection).unwrap();
    let lease = selection.native_credential(&verifier).await.unwrap();
    let input = NowPublisherInput {
        authority: &authority,
        caller: &caller,
        scope: native_scope,
        heads: heads(),
    };
    let mut cancelled = start_now_publisher(&transport, &lease, original, input, wire())
        .await
        .unwrap();
    signal.cancel();
    assert!(matches!(
        cancelled.next().await,
        Err(NowPublisherError::Request(_))
    ));
    assert!(matches!(
        cancelled.next().await,
        Err(NowPublisherError::Closed(1000))
    ));
    cancelled.shutdown().await.unwrap();
}

#[tokio::test]
#[ignore = "real DaemonAuth/Core PROJECT/Lance compositor, Broker TCP WS and PostgreSQL TLS NOSUPERUSER"]
async fn now_publisher_broker_postgres() {
    let fixture = pg::Fixture::new(true).await;
    let mut connection = fixture.connection().await;
    connection.execute("CREATE ROLE runtime_now_owner LOGIN NOSUPERUSER PASSWORD 'ephemeral-owner'; DROP SCHEMA public CASCADE; CREATE SCHEMA public AUTHORIZATION runtime_now_owner; GRANT CONNECT,TEMP ON DATABASE postgres TO runtime_now_owner").await.unwrap();
    let store = Arc::new(
        PostgresControlStore::open(pg_config(&fixture), pg::digester(), pg::deadline())
            .await
            .unwrap(),
    );
    assert!(
        !sqlx_core::query_scalar::query_scalar::<_, bool>(
            "SELECT rolsuper FROM pg_roles WHERE rolname='runtime_now_owner'"
        )
        .fetch_one(&mut connection)
        .await
        .unwrap()
    );
    let capabilities = [
        "presence:read",
        "presence:publish",
        "task:read",
        "memory:read",
        "knowledge:read",
    ];

    let key = Arc::new(
        Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let keys = json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(key.public_key().as_ref()),"kid":"actual","alg":"EdDSA","use":"sig","key_ops":["verify"]}]});
    let jwks = serde_json::from_value(keys.clone()).unwrap();
    let verifier =
        Arc::new(BrokerAccessVerifier::new(&origin, "broker-api", None, &jwks, n(8192)).unwrap());
    let claims = json!({"iss":origin,"sub":"fixture-sub","aud":["broker-api"],"client_id":"graphit-cli","jti":"runtime-now","token_use":"access","scope":"presence-sdk","iat":seconds()-1,"exp":seconds()+3600});
    let message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(json!({"alg":"EdDSA","kid":"actual","typ":"JWT"}).to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let raw = Zeroizing::new(format!(
        "{message}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(message.as_bytes()).as_ref())
    ));
    let metadata = json!({"version":"1","issuer":origin,"authentication":{"type":"oidc","issuer":origin,"client_id":"graphit-cli","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["broker-api"],"access_token_audience":"broker-api","mcp_resources":[]}});
    let discovery = json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),"token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/keys"),"userinfo_endpoint":format!("{origin}/userinfo"),"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]});
    let runtime = RealtimeServerRuntime::new(Duration::from_secs(4), n(8)).unwrap();
    let routes = broker_realtime_router(store.clone(), verifier.clone(), realtime(), &runtime)
        .unwrap()
        .merge(
            broker_instance_inventory_router(
                store.clone(),
                verifier,
                BrokerPublishHttpConfig {
                    required_scopes: vec!["presence-sdk".into()],
                    grants_json_bytes: n(65536),
                    body_bytes: n(65536),
                    request_timeout: Duration::from_secs(2),
                },
                BrokerOwnReadHttpConfig {
                    required_scopes: vec!["presence-sdk".into()],
                    grants_json_bytes: n(65536),
                    inventory_json_bytes: n(65536),
                    request_timeout: Duration::from_secs(2),
                },
            )
            .unwrap(),
        )
        .route(
            "/.well-known/graphit-broker",
            axum::routing::get(move || async move { axum::Json(metadata) }),
        )
        .route(
            "/.well-known/openid-configuration",
            axum::routing::get(move || async move { axum::Json(discovery) }),
        )
        .route(
            "/keys",
            axum::routing::get(move || async move { axum::Json(keys) }),
        );
    let native_server = Server::start(listener, routes);
    connection.execute("INSERT INTO public.local_users(username,subject,identity_kind,password_hash,name,email,organization,teams_json,enabled,password_change_required,revision,created_at,updated_at) VALUES('fixture','fixture-local','human','','Fixture','','','[]',1,0,1,0,0)").await.unwrap();
    let digest = pg::digester()
        .token(TokenKind::LocalAccess, LOCAL_RAW.as_bytes())
        .unwrap();
    sqlx_core::query::query("INSERT INTO public.local_tokens(token_hash,token_id,token_kind,subject,oidc_subject,principal_json,client_id,audience,resource,scopes_json,local_user_revision,family_id,auth_time,expires_at,created_at) VALUES($1,'runtime-local','access','fixture-local','local',$2,'graphit-cli','explicit-audience','','[\"presence-sdk\"]',1,'',0,$3,0)")
        .bind(digest.to_sql_hex()).bind(json!({"issuer":"local","subject":"fixture-local","username":"fixture","auth_method":"fixture","local_user_revision":1}).to_string())
        .bind(i64::try_from(seconds()*1_000_000).unwrap()+300_000_000).execute(&mut connection).await.unwrap();
    let local_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_server = Server::start(
        local_listener,
        local_realtime_router(
            store.clone(),
            LocalRealtimeHttpConfig {
                audience: "explicit-audience".into(),
                identity_json_bytes: n(16384),
                realtime: realtime(),
            },
            &runtime,
        )
        .unwrap(),
    );
    let signal = Cancellation::default();
    let original = request(&signal, Duration::from_secs(180));
    let auth = AuthFixture::new(
        &origin,
        Credential::Native(raw.to_string()),
        None,
        &ordinary(),
        &original,
    );
    let local_auth = AuthFixture::new(
        &local_server.origin,
        Credential::Local,
        Some(auth.project_paths.root()),
        &ordinary(),
        &original,
    );
    assert_eq!(auth.project_id(), local_auth.project_id());
    let mut project_ids = vec![auth.project_id()];
    acl(&mut connection, &capabilities, &project_ids).await;

    let core_inventory = GlobalProjectInventory::at(&auth.paths, 65536)
        .unwrap()
        .load(original.snapshot())
        .unwrap();
    let maintained = core_inventory.get(auth.project_id()).unwrap();
    assert!(maintained.versions().any(|version| version == "main"));
    assert!(
        maintained
            .versions()
            .any(|version| version == "branch/ação")
    );
    assert!(
        auth.store
            .load(&original)
            .unwrap()
            .document()
            .provider("now")
            .is_some()
    );

    let http = BoundedClient::new(
        ClientPolicy {
            origin: &origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network(),
    )
    .unwrap();
    register_native(
        &http,
        &origin,
        &raw,
        &auth,
        &original,
        &["main", "branch/ação"],
    )
    .await;
    store
        .local_instance_registrar("explicit-audience", n(16384), n(65536))
        .unwrap()
        .register(
            LOCAL_RAW,
            &["presence-sdk".into()],
            &InstanceInventory::new(
                local_auth.instance_id().into(),
                vec![
                    InstanceProjectVersions::new(
                        auth.project_id().to_string(),
                        vec!["main".into(), "branch/ação".into()],
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
            pg::deadline(),
        )
        .await
        .unwrap();
    channels_and_lifecycle(
        &auth,
        &local_auth,
        &original,
        (&origin, &local_server.origin, &raw),
        &mut connection,
        &project_ids,
        &capabilities,
    )
    .await;
    corpus_section_matrix(
        &origin,
        &raw,
        &original,
        &mut connection,
        &mut project_ids,
        &capabilities,
    )
    .await;
    registered_owner_conflict(
        &origin,
        &raw,
        &key,
        &original,
        &mut connection,
        &mut project_ids,
        &capabilities,
    )
    .await;
    ordinary_and_service_lifetimes(&auth, &origin).await;
    signed_expiry_without_original_deadline(
        &origin,
        &key,
        &original,
        &mut connection,
        &mut project_ids,
        &capabilities,
    )
    .await;
    original_cancellation(&auth, &origin, &original, &signal).await;
    let cleanup_signal = Cancellation::default();
    let cleanup = request(&cleanup_signal, Duration::from_secs(10));
    assert!(
        auth.auth
            .remove_runtime(cleanup.snapshot())
            .failure()
            .is_none()
    );
    assert!(
        local_auth
            .auth
            .remove_runtime(cleanup.snapshot())
            .failure()
            .is_none()
    );
    runtime.shutdown().await.unwrap();
    local_server.shutdown().await;
    native_server.shutdown().await;
    store.shutdown(pg::deadline()).await.unwrap();
    drop(store);
    connection.close().await.unwrap();
    fixture.shutdown().unwrap();
}
