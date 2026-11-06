#![cfg(target_os = "linux")]

#[expect(
    dead_code,
    reason = "Mature PostgreSQL support also serves other native Broker control journeys"
)]
mod pg {
    include!(concat!(
        env!("GRAPHIT_BROKER_CONTROL_SOURCE"),
        "/tests/support/mod.rs"
    ));
}

use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use axum::http::{HeaderMap, HeaderValue, Method, header};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use broker_control::{
    BrokerAccessVerifier, ConnectionLimits, PostgresConfig, PostgresControlStore,
    PostgresCredentials, PostgresEndpoint,
};
use broker_presence::{
    BrokerOwnReadHttpConfig, BrokerPublishHttpConfig, InstanceInventoryListHttpConfig,
    InstanceInventoryReadHttpConfig, InstanceRegistryHttpConfig, LocalRealtimeHttpConfig,
    RealtimeHttpConfig, RealtimeServerRuntime, broker_instance_inventory_router,
    broker_realtime_router, instance_inventory_router, local_realtime_router,
};
use broker_security::TokenKind;
use graphit_auth::{
    AdminSessionPolicy, CredentialSource, ExternalEffect, IssuerAuthorityPolicy, IssuerLimits,
    Permission, ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation,
    RemoteServiceBindingInput, ScopeCapture, ScopeInput, ServiceAdmissionLimits,
    ServiceAuthorityInput, ServiceOrigin, ServicePurpose, ServiceRuleInput, SourceAuthorityPolicy,
    TtlPolicy,
    broker::{JsonLimits, VerifierLimits},
    profile::{
        AiService, BrokerService, LocalCredentials, LocalDevice, LocalExecution, OidcTokens,
        ProfileCredentials, ProfileData, ProfileExpectation, ProfileLimits, ProviderConfig,
        ProviderKind, S3Location, S3Source, Secret,
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
    now_subscription::{
        NowRemoteCode, NowSubscription, NowSubscriptionConfig, RemoteNowSnapshot,
        start_now_subscription,
    },
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
    websocket::{CloseCode, Message, WebSocketClient, WebSocketLimits, WebSocketRead},
};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{
        CreateMemory, MemoryHeadRead, MemoryHeadReadLimits, MemoryStoreReadLimits,
        MemoryStoreWriteLimits, NativeMemoryStore,
    },
    reference_projection::AuthoredReferencesField,
    task_spec::{CheckKind, TaskId},
    task_store::{
        CreateCheck, CreateSession, CreateTask, NativeTaskStore, SessionId, TaskStoreReadLimits,
        TaskStoreWriteLimits,
    },
};

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
    Renewable {
        access: String,
        refresh: Option<String>,
        id: String,
        expires_at: i64,
    },
    Local,
}
struct AuthFixture {
    auth: DaemonAuth,
    paths: InstallationPaths,
    project_paths: ProjectPaths,
    project: CapturedPublicationScope,
    project_record: ScopeBindingRecord,
    version: VersionContext,
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
        Self::with_embedding(
            endpoint,
            credential,
            project_root,
            permissions,
            AiService::Broker,
            original,
        )
    }
    fn with_embedding(
        endpoint: &str,
        credential: Credential,
        project_root: Option<&std::path::Path>,
        permissions: &[Permission],
        embedding: AiService,
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
        let native = matches!(
            &credential,
            Credential::Native(_) | Credential::Renewable { .. }
        );
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
        let mut rules = vec![
            rule(ServicePurpose::PresencePublish),
            rule(ServicePurpose::PresenceRead),
        ];
        if let AiService::Local(execution) = embedding {
            rules.push(ServiceRuleInput::Local {
                origin: ServiceOrigin::Runtime,
                provider: "now",
                purpose: ServicePurpose::Embedding,
                execution,
            });
        }

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
                        embedding,
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
                    Credential::Renewable {
                        access,
                        refresh,
                        id,
                        expires_at,
                    } => ProfileCredentials::Broker {
                        issuer: endpoint.into(),
                        subject: "fixture-sub".into(),
                        s3_disabled: true,
                        tokens: OidcTokens {
                            access_token: Secret::new(SecretString::from(access.clone())),
                            refresh_token: refresh
                                .as_ref()
                                .map(|raw| Secret::new(SecretString::from(raw.clone()))),
                            id_token: Secret::new(SecretString::from(id.clone())),
                            expires_at: *expires_at,
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

struct ReleasedInstallation {
    paths: InstallationPaths,
    project_paths: ProjectPaths,
    project_id: graphit_core::project_identity::ProjectId,
    instance_id: String,
    _directory: tempfile::TempDir,
}
impl ReleasedInstallation {
    fn project_id(&self) -> graphit_core::project_identity::ProjectId {
        self.project_id
    }
    fn instance_id(&self) -> &str {
        &self.instance_id
    }
}
impl AuthFixture {
    fn release(self, original: &RequestContext<'_>) -> ReleasedInstallation {
        let project_id = self.project_id();
        let instance_id = self.instance_id().to_owned();
        let outcome = self.auth.remove_runtime(original.snapshot());
        assert!(outcome.failure().is_none() && outcome.cleanup_failure().is_none());
        let Self {
            auth,
            paths,
            project_paths,
            _directory,
            ..
        } = self;
        // Runtime key removal does not release the retained singleton owner.
        // Drop that real owner before constructing a new Foreground on this install.
        drop(auth);
        ReleasedInstallation {
            paths,
            project_paths,
            project_id,
            instance_id,
            _directory,
        }
    }
}

struct Server {
    origin: String,
    app: axum::Router,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl Server {
    fn start(listener: TcpListener, app: axum::Router) -> Self {
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (stop, rx) = tokio::sync::oneshot::channel();
        let serving = app.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, serving)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });
        Self {
            origin,
            app,
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
    project_acl(connection, caps, projects, None).await;
}

async fn project_acl(
    connection: &mut PgConnection,
    caps: &[&str],
    projects: &[graphit_core::project_identity::ProjectId],
    denied_publisher: Option<graphit_core::project_identity::ProjectId>,
) {
    connection.execute("BEGIN; DELETE FROM public.resource_grants; UPDATE public.resource_acl_state SET revision=revision+1,updated_at=0 WHERE id=1").await.unwrap();
    for (index, project) in projects.iter().enumerate() {
        let grant = format!("runtime-now-{index}");
        sqlx_core::query::query("INSERT INTO public.resource_grants(id,name,access_kind,principal,s3_route,created_at,updated_at) VALUES($1,'fixture','authenticated','','',0,0)")
            .bind(&grant).execute(&mut *connection).await.unwrap();
        for cap in caps {
            if Some(*project) == denied_publisher && *cap == "presence:publish" {
                continue;
            }
            sqlx_core::query::query(
                "INSERT INTO public.grant_capabilities(grant_id,value) VALUES($1,$2)",
            )
            .bind(&grant)
            .bind(*cap)
            .execute(&mut *connection)
            .await
            .unwrap();
        }
        sqlx_core::query::query("INSERT INTO public.grant_projects(grant_id,value) VALUES($1,$2)")
            .bind(&grant)
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
) -> (TaskId, SessionId) {
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
    (saved.specification.id, session.id)
}
async fn add_memory<'a>(
    auth: &'a DaemonAuth,
    caller: &'a graphit_auth::AuthenticatedCaller<'a>,
    original: &'a RequestContext<'a>,
    scope: ScopeInput<'a>,
    name: &str,
) -> graphit_records::memory_store::Memory {
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
    .unwrap()
}

use graphit_app::foreground_source::{ForegroundFailure, ForegroundSource};
use graphit_core::config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource, ConfigStore};
use graphit_daemon::model_service as model_api;

async fn assert_local_source_operational(
    paths: &InstallationPaths,
    address: std::net::SocketAddr,
    project: &ProjectPaths,
    original: &RequestContext<'_>,
) {
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let status = graphit_app::daemon_control::request(
        paths,
        address,
        &client,
        native_limits(),
        original,
        graphit_app::daemon_control::Operation::Status,
        None,
    )
    .await
    .unwrap();
    assert_eq!(status.state, graphit_app::daemon_control::State::Running);
    assert_eq!(status.pid, std::process::id());
    let arguments = json!({
        "ai_optimized": false,
        "selection": {"project_dir": project.root().to_str().unwrap()},
        "input": {"selection": {"table": "tasks", "status": null},
                  "columns": ["id"], "page_size": 1, "top_k": 1}
    });
    let prepared = rmcp::model::CallToolRequestParams::new("graphit_task_query")
        .with_arguments(arguments.as_object().unwrap().clone());
    let mut output = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(10),
        graphit_app::task_entry::run(
            paths,
            &client,
            prepared,
            (native_limits(), original.content_limits()),
            original,
            &mut output,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["operation"], "query");
    assert!(matches!(
        value["result"]["state"].as_str(),
        Some("published" | "no_corpus")
    ));
    if value["result"]["state"] == "published" {
        let rows = value["result"]["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "query must return a real seeded Task");
        assert!(!rows[0][0].as_str().unwrap().is_empty());
    }
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_graphit"))
            .current_dir(project.root())
            .env_remove("GRAPHIT_GLOBAL_DIR")
            .arg("--non-interactive")
            .arg("--global-dir")
            .arg(paths.root())
            .args(["daemon", "status"])
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "CLI administrative status must survive Now failure"
    );
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["pid"], std::process::id());
    assert_eq!(status["state"], "running");
}

use crate::model_fixture;

async fn register_native(
    endpoint: (&str, &str),
    instance_id: &str,
    original: &RequestContext<'_>,
    projects: serde_json::Value,
) {
    let (origin, raw) = endpoint;
    let client = BoundedClient::new(
        ClientPolicy {
            origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network(),
    )
    .unwrap();
    let mut authorization = HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
    authorization.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, authorization);
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let response = client
        .execute(
            NetworkRequest {
                method: Method::PUT,
                url: &format!("{origin}/v1/instances/{instance_id}/inventory"),
                headers: &headers,
                body: json!({"projects":projects}).to_string().as_bytes(),
            },
            NetworkRequestContext::with_deadline(original.deadline(), original.cancellation()),
        )
        .await
        .unwrap();
    assert!(response.status().is_success());
}

async fn observe_native_admission_lock(connection: &mut PgConnection) {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            connection.execute("SELECT pg_stat_clear_snapshot()").await.unwrap();
            let blocked = sqlx_core::query_scalar::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE usename='runtime_now_owner' AND wait_event_type='Lock' AND query LIKE '%resource_acl_state%')"
            ).fetch_one(&mut *connection).await.unwrap();
            if blocked { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("real Broker channel admission did not reach the held SQL lock");
}

async fn seed_foreground_projects(
    first: &AuthFixture,
    second: &AuthFixture,
    original: &RequestContext<'_>,
) -> [(TaskId, SessionId); 2] {
    let first_caller = first
        .auth
        .issuer()
        .authenticate_runtime(&first.runtime_key(original), original)
        .unwrap();
    let second_caller = second
        .auth
        .issuer()
        .authenticate_runtime(&second.runtime_key(original), original)
        .unwrap();
    let first_heads = seed(
        &first.auth,
        &first_caller,
        original,
        first.scope(&first.version),
        "first",
    )
    .await;
    let second_heads = seed(
        &second.auth,
        &second_caller,
        original,
        second.scope(&second.version),
        "second",
    )
    .await;
    [first_heads, second_heads]
}

async fn foreground_channels_and_lifecycle(
    seeded_first: (AuthFixture, [(TaskId, SessionId); 2]),
    second: &AuthFixture,
    writer: &AuthFixture,
    records: &AuthFixture,
    original: &RequestContext<'_>,
    connection: &mut PgConnection,
    capabilities: &[&str],
    endpoint: (&str, &str),
) -> ReleasedInstallation {
    let (first, seeded_heads) = seeded_first;
    let (origin, _) = endpoint;
    let ids = [first.project_id(), second.project_id()];
    assert_ne!(ids[0], ids[1]);
    assert_ne!(first.project_paths.root(), second.project_paths.root());
    acl(connection, capabilities, &ids).await;
    let first = first.release(original);
    let config = ConfigSnapshot::new(
        first.paths.brand(),
        [ConfigDocument::new(
            first.paths.brand(),
            ConfigSource::Command,
            [(
                ConfigKey::new("broker.allow_loopback_http").unwrap(),
                json!(true),
            )],
        )
        .unwrap()],
    )
    .unwrap();
    let inputs = [
        ProjectBootstrapInput {
            project: &first.project_paths,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        },
        ProjectBootstrapInput {
            project: &second.project_paths,
            explicit_version: "branch/ação",
            pin: None,
            detached_base: "",
        },
    ];
    let owner = ForegroundSource::start_and_publish(
        &first.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        original,
    )
    .await
    .unwrap();
    let address = owner.address().unwrap();
    let service = Cancellation::default();
    // Inventory is already published. Native upgrade verification has no ACL
    // read; this physical lock blocks admission after publish, before welcome.
    connection
        .execute("BEGIN; LOCK TABLE public.resource_acl_state IN ACCESS EXCLUSIVE MODE")
        .await
        .unwrap();
    let operational = service.clone();
    let mut running = tokio::spawn(async move { owner.run(&operational).await });
    tokio::select! {
        () = observe_native_admission_lock(connection) => (),
        result = &mut running => panic!("Foreground ended before real admission lock: {result:?}"),
    }
    assert!(!running.is_finished());
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let status = tokio::select! {
        status = graphit_app::daemon_control::request(
            &first.paths,
            address,
            &client,
            native_limits(),
            original,
            graphit_app::daemon_control::Operation::Status,
            None,
        ) => status.unwrap(),
        result = &mut running => panic!("Foreground ended while HTTP awaited pending Now: {result:?}"),
    };
    assert_eq!(status.state, graphit_app::daemon_control::State::Running);
    assert_eq!(status.pid, std::process::id());
    // HTTP is serving while the held admission lock still prevents Now welcome.
    assert!(!running.is_finished());
    connection.execute("COMMIT").await.unwrap();
    let caller = writer
        .auth
        .issuer()
        .authenticate_runtime(&writer.runtime_key(original), original)
        .unwrap();
    let mut selection = writer
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "now")
        .unwrap();
    let cache = cache();
    let verifier = cache.get(&mut selection).unwrap();
    let lease = selection.native_credential(&verifier).await.unwrap();
    let transport = websocket(origin);
    let make = |id: graphit_core::project_identity::ProjectId| NowSubscriptionConfig {
        project_id: id,
        instance_id: Some(first.instance_id()),
        version: None,
        command_bytes: n(65536),
        snapshot_bytes: n(65536),
    };
    let mut a = native_subscription(
        &transport,
        &lease,
        original,
        make(ids[0]),
        &service,
        &mut running,
    )
    .await;
    let mut b = native_subscription(
        &transport,
        &lease,
        original,
        make(ids[1]),
        &service,
        &mut running,
    )
    .await;
    let mut initial_observations = Vec::with_capacity(2);
    for ((subscriber, expected_version, expected_title), (task_id, session_id)) in
        [(&mut a, "main", "first"), (&mut b, "branch/ação", "second")]
            .into_iter()
            .zip(seeded_heads)
    {
        loop {
            let snapshot =
                native_snapshot(subscriber, "initial snapshot", &service, &mut running).await;
            if let Some(instance) = snapshot.instances().first() {
                assert_eq!(instance.instance_id(), first.instance_id());
                assert_eq!(instance.version(), expected_version);
                let value = serde_json::to_value(instance.now()).unwrap();
                let generated_at = instance.now().generated_at().parse::<u128>().unwrap();
                assert!(generated_at > 9_007_199_254_740_991);
                // Source grants Read(Task); seed created and claimed one linked
                // Task/session pair. This journey does not install a Task denial.
                for section in ["tasks", "sessions", "memories", "knowledge"] {
                    assert_eq!(value["sections"][section]["availability"], "available");
                    assert_eq!(value["sections"][section]["total"], 1);
                    assert_eq!(value["sections"][section]["has_more"], false);
                    assert!(value["sections"][section]["error"].is_null());
                    assert_eq!(
                        value["sections"][section]["items"]
                            .as_array()
                            .unwrap()
                            .len(),
                        1
                    );
                }
                let task = &value["sections"]["tasks"]["items"][0];
                let session = &value["sections"]["sessions"]["items"][0];
                assert_eq!(task["id"], serde_json::to_value(task_id).unwrap());
                assert_eq!(session["id"], serde_json::to_value(session_id).unwrap());
                assert_eq!(task["session_id"], session["id"]);
                for item in [task, session] {
                    assert_eq!(item["title"], expected_title);
                    assert!(!item["owner"].as_str().unwrap().is_empty());
                    assert!(
                        item["lease_expires_at"]
                            .as_str()
                            .unwrap()
                            .parse::<u128>()
                            .unwrap()
                            > generated_at
                    );
                }
                assert_eq!(session["completed_tasks"], 0);
                assert_eq!(session["total_tasks"], 1);
                assert_eq!(value["sections"]["memories"]["total"], 1);
                assert_eq!(value["sections"]["knowledge"]["total"], 1);
                assert_eq!(
                    value["sections"]["knowledge"]["items"][0]["updated_at"],
                    "9007199254740993"
                );
                assert!(!serde_json::to_string(&value).unwrap().contains("PRIVATE_"));
                initial_observations.push(value);
                break;
            }
        }
    }
    let response = reqwest::Client::new()
        .get(format!("http://{address}/"))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_client_error());
    let peer = tokio::net::TcpStream::connect(address).await.unwrap();
    drop(peer); // A real client EOF must not stop the Foreground owner.
    let after_eof = reqwest::Client::new()
        .get(format!("http://{address}/"))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();
    assert!(after_eof.status().is_client_error());
    assert!(!running.is_finished());
    let records_caller = records
        .auth
        .issuer()
        .authenticate_runtime(&records.runtime_key(original), original)
        .unwrap();
    add_memory(
        &records.auth,
        &records_caller,
        original,
        records.scope(&records.version),
        "fresh-head",
    )
    .await;
    let mut previous = initial_observations.remove(0);
    let initial_generated_at = previous["generated_at"]
        .as_str()
        .unwrap()
        .parse::<u128>()
        .unwrap();
    let mut repeated_observation = false;
    loop {
        let snapshot = native_snapshot(&mut a, "fresh head", &service, &mut running).await;
        if let Some(instance) = snapshot.instances().first() {
            let now = serde_json::to_value(instance.now()).unwrap();
            let generated_at = instance.now().generated_at().parse::<u128>().unwrap();
            let previous_generated_at = previous["generated_at"]
                .as_str()
                .unwrap()
                .parse::<u128>()
                .unwrap();
            assert!(generated_at >= previous_generated_at);
            if generated_at == previous_generated_at {
                assert_eq!(now, previous);
                repeated_observation = true;
            }
            if now["sections"]["memories"]["total"] == 2 {
                assert_eq!(now["sections"]["memories"]["has_more"], false);
                let items = now["sections"]["memories"]["items"].as_array().unwrap();
                assert_eq!(items.len(), 2);
                let authority = ProjectRecordAuthority::new(records.auth.issuer());
                let admission = authority
                    .admit_authenticated_read(
                        &records_caller,
                        original,
                        records.scope(&records.version),
                        ProjectReadModule::Memory,
                    )
                    .unwrap();
                let MemoryHeadRead::Published { mut heads, .. } = NativeMemoryStore::observe_heads(
                    original.snapshot(),
                    &admission,
                    heads().memory,
                )
                .await
                .unwrap() else {
                    panic!("real SDK corpus expected");
                };
                heads.sort_by(|a, b| {
                    b.updated_at
                        .cmp(&a.updated_at)
                        .then_with(|| a.id.cmp(&b.id))
                });
                let expected = heads
                    .iter()
                    .map(|head| head.title.as_str())
                    .collect::<Vec<_>>();
                let names = items
                    .iter()
                    .map(|item| item["title"].as_str().unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(names, expected);
                assert!(generated_at > initial_generated_at);
                eprintln!(
                    "Now sequence observed initial_generated_at={initial_generated_at} completed_generated_at={generated_at} repeated_observation={repeated_observation}"
                );
                break;
            }
            previous = now;
        }
    }
    service.cancel();
    tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    for subscriber in [&mut a, &mut b] {
        loop {
            if subscriber.next().await.unwrap().instances().is_empty() {
                break;
            }
        }
    }
    assert!(!first.paths.root().join("daemon/mcp.json").exists());
    assert!(!first.paths.root().join("daemon/mcp.key").exists());
    let restarted_inputs = [
        ProjectBootstrapInput {
            project: &first.project_paths,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        },
        ProjectBootstrapInput {
            project: &second.project_paths,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        },
    ];
    let restarted = ForegroundSource::start_and_publish(
        &first.paths,
        &restarted_inputs,
        1,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        original,
    )
    .await
    .unwrap();
    let restarted_address = restarted.address().unwrap();
    let restart_stop = Cancellation::default();
    let operational = restart_stop.clone();
    let running = tokio::spawn(async move { restarted.run(&operational).await });
    for subscriber in [&mut a, &mut b] {
        loop {
            let snapshot = subscriber.next().await.unwrap();
            if snapshot
                .instances()
                .iter()
                .any(|instance| instance.version() == "main")
            {
                break;
            }
        }
    }
    // An independent, actually verified Native caller publishes on the same
    // registered channel using the complete snapshot produced by Foreground.
    let actual = loop {
        let snapshot = a.next().await.unwrap();
        if let Some(instance) = snapshot.instances().first() {
            break serde_json::to_value(instance.now()).unwrap();
        }
    };
    let mut authorization = HeaderValue::from_str(&format!("Bearer {}", endpoint.1)).unwrap();
    authorization.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, authorization);
    let mut replacement = transport
        .connect(
            &format!("{origin}/v1/realtime"),
            headers,
            NetworkRequestContext::with_deadline(original.deadline(), original.cancellation()),
        )
        .await
        .unwrap();
    replacement
        .send(Message::Text(
            json!({"op":"publish","instance_id":first.instance_id(),
        "project_id":ids[0],"version":"main","snapshot":actual})
            .to_string(),
        ))
        .await
        .unwrap();
    let welcome = replacement.receive().await.unwrap();
    let WebSocketRead::Message(Message::Text(welcome)) = welcome else {
        panic!("actual Broker welcome expected");
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&welcome).unwrap()["type"],
        "welcome"
    );
    replacement.close(CloseCode::Normal).await.unwrap();
    while !a.next().await.unwrap().instances().is_empty() {}
    assert!(
        !running.is_finished(),
        "Superseded must retire only its publisher"
    );
    assert_local_source_operational(
        &first.paths,
        restarted_address,
        &first.project_paths,
        original,
    )
    .await;
    restart_stop.cancel();
    tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    for subscriber in [&mut a, &mut b] {
        loop {
            if subscriber.next().await.unwrap().instances().is_empty() {
                break;
            }
        }
    }
    assert!(!first.paths.root().join("daemon/mcp.json").exists());
    assert!(!first.paths.root().join("daemon/mcp.key").exists());
    let acl_owner = ForegroundSource::start_and_publish(
        &first.paths,
        &restarted_inputs,
        1,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        original,
    )
    .await
    .unwrap();
    let acl_address = acl_owner.address().unwrap();
    let acl_stop = Cancellation::default();
    let operational = acl_stop.clone();
    let running = tokio::spawn(async move { acl_owner.run(&operational).await });
    for subscriber in [&mut a, &mut b] {
        loop {
            if !subscriber.next().await.unwrap().instances().is_empty() {
                break;
            }
        }
    }
    // The actual Broker denies publication for the first PROJECT only.
    project_acl(connection, capabilities, &ids, Some(ids[0])).await;
    while !a.next().await.unwrap().instances().is_empty() {}
    assert!(
        !running.is_finished(),
        "Forbidden must not terminate local Source"
    );
    assert_local_source_operational(&first.paths, acl_address, &first.project_paths, original)
        .await;
    let sibling_caller = second
        .auth
        .issuer()
        .authenticate_runtime(&second.runtime_key(original), original)
        .unwrap();
    let fresh = add_memory(
        &second.auth,
        &sibling_caller,
        original,
        second.scope(&second.version),
        "sibling-after-publisher-denial",
    )
    .await;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let snapshot = b.next().await.unwrap();
            if snapshot.instances().iter().any(|instance| {
                let now = serde_json::to_value(instance.now()).unwrap();
                now["sections"]["memories"]["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|memory| memory["id"] == serde_json::to_value(fresh.id).unwrap())
            }) {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(
        !running.is_finished(),
        "the healthy sibling must publish new heads"
    );
    acl_stop.cancel();
    tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!first.paths.root().join("daemon/mcp.json").exists());
    assert!(!first.paths.root().join("daemon/mcp.key").exists());
    for subscriber in [&mut a, &mut b] {
        loop {
            if subscriber.next().await.unwrap().instances().is_empty() {
                break;
            }
        }
    }
    acl(connection, capabilities, &ids).await;
    let withdrawn = ForegroundSource::start_and_publish(
        &first.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        original,
    )
    .await
    .unwrap();
    let withdrawn_address = withdrawn.address().unwrap();
    let withdrawal_stop = Cancellation::default();
    let operational = withdrawal_stop.clone();
    let running = tokio::spawn(async move { withdrawn.run(&operational).await });
    for (subscriber, version) in [(&mut a, "main"), (&mut b, "branch/ação")] {
        loop {
            let snapshot = subscriber.next().await.unwrap();
            if snapshot
                .instances()
                .iter()
                .any(|instance| instance.version() == version)
            {
                break;
            }
        }
    }
    register_native(
        endpoint,
        first.instance_id(),
        original,
        json!([{"project_id":ids[0],"versions":["main"]}]),
    )
    .await;
    while !b.next().await.unwrap().instances().is_empty() {}
    assert!(
        !running.is_finished(),
        "withdrawal must retire only the affected publisher"
    );
    assert_local_source_operational(
        &first.paths,
        withdrawn_address,
        &first.project_paths,
        original,
    )
    .await;
    withdrawal_stop.cancel();
    tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!first.paths.root().join("daemon/mcp.json").exists());
    assert!(!first.paths.root().join("daemon/mcp.key").exists());
    for subscriber in [&mut a, &mut b] {
        loop {
            if subscriber.next().await.unwrap().instances().is_empty() {
                break;
            }
        }
    }
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
    let pending = ForegroundSource::start_and_publish(
        &first.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        original,
    )
    .await
    .unwrap();
    connection
        .execute("BEGIN; LOCK TABLE public.resource_acl_state IN ACCESS EXCLUSIVE MODE")
        .await
        .unwrap();
    let pending_stop = Cancellation::default();
    let operational = pending_stop.clone();
    let mut running = tokio::spawn(async move { pending.run(&operational).await });
    tokio::select! {
        () = observe_native_admission_lock(connection) => (),
        result = &mut running => panic!("Foreground ended before cancellation gate: {result:?}"),
    }
    assert!(!running.is_finished());
    pending_stop.cancel();
    let stopped = tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .unwrap()
        .unwrap();
    connection.execute("COMMIT").await.unwrap();
    stopped.unwrap();
    assert!(!first.paths.root().join("daemon/mcp.json").exists());
    assert!(!first.paths.root().join("daemon/mcp.key").exists());
    first
}

async fn foreground_signed_expiry_and_no_corpus(
    origin: &str,
    key: &Ed25519KeyPair,
    writer: &AuthFixture,
    original: &RequestContext<'_>,
    connection: &mut PgConnection,
    capabilities: &[&str],
    existing: &[graphit_core::project_identity::ProjectId],
) {
    let claims = json!({"iss":origin,"sub":"fixture-sub","aud":["broker-api"],"client_id":"graphit-cli",
        "jti":"foreground-short-signed","token_use":"access","scope":"presence-sdk","iat":seconds()-1,"exp":seconds()+10});
    let message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(json!({"alg":"EdDSA","kid":"actual","typ":"JWT"}).to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let raw = Zeroizing::new(format!(
        "{message}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(message.as_bytes()).as_ref())
    ));
    let auth = AuthFixture::new(
        origin,
        Credential::Native(raw.to_string()),
        None,
        &ordinary(),
        original,
    );
    let mut projects = existing.to_vec();
    projects.push(auth.project_id());
    acl(connection, capabilities, &projects).await;
    let auth = auth.release(original);
    let config = ConfigSnapshot::new(
        auth.paths.brand(),
        [ConfigDocument::new(
            auth.paths.brand(),
            ConfigSource::Command,
            [(
                ConfigKey::new("broker.allow_loopback_http").unwrap(),
                json!(true),
            )],
        )
        .unwrap()],
    )
    .unwrap();
    let inputs = [ProjectBootstrapInput {
        project: &auth.project_paths,
        explicit_version: "main",
        pin: None,
        detached_base: "",
    }];
    let owner = ForegroundSource::start_and_publish(
        &auth.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        original,
    )
    .await
    .unwrap();
    let service = Cancellation::default();
    let operational = service.clone();
    let running = tokio::spawn(async move { owner.run(&operational).await });
    let caller = writer
        .auth
        .issuer()
        .authenticate_runtime(&writer.runtime_key(original), original)
        .unwrap();
    let mut selection = writer
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "now")
        .unwrap();
    let cache = cache();
    let verifier = cache.get(&mut selection).unwrap();
    let lease = selection.native_credential(&verifier).await.unwrap();
    let transport = websocket(origin);
    let mut subscriber = start_now_subscription(
        &transport,
        &lease,
        original,
        NowSubscriptionConfig {
            project_id: auth.project_id(),
            instance_id: Some(auth.instance_id()),
            version: None,
            command_bytes: n(65536),
            snapshot_bytes: n(65536),
        },
    )
    .await
    .unwrap();
    loop {
        let snapshot = subscriber.next().await.unwrap();
        if let Some(instance) = snapshot.instances().first() {
            let now = serde_json::to_value(instance.now()).unwrap();
            for section in ["tasks", "sessions", "memories", "knowledge"] {
                if section == "knowledge" {
                    // Source may already have committed its empty document
                    // generation. Empty published and absent are distinct.
                    assert!(matches!(
                        now["sections"][section]["availability"].as_str(),
                        Some("no_corpus" | "available")
                    ));
                } else {
                    assert_eq!(now["sections"][section]["availability"], "no_corpus");
                }
                assert_eq!(now["sections"][section]["total"], 0);
                assert_eq!(now["sections"][section]["has_more"], false);
                assert!(now["sections"][section]["error"].is_null());
                assert!(
                    now["sections"][section]["items"]
                        .as_array()
                        .unwrap()
                        .is_empty()
                );
            }
            break;
        }
    }
    let failure = tokio::time::timeout(Duration::from_secs(20), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    let terminal = failure.service.as_ref().unwrap_or(&failure.error);
    let cause = terminal.get_ref().unwrap();
    assert!(
        matches!(
            cause.downcast_ref::<graphit_daemon::now_publisher::NowPublisherError>(),
            Some(
                graphit_daemon::now_publisher::NowPublisherError::Admission(_)
                    | graphit_daemon::now_publisher::NowPublisherError::Remote(
                        NowRemoteCode::Unauthenticated
                    )
                    | graphit_daemon::now_publisher::NowPublisherError::Closed(1008)
            )
        ) || cause
            .downcast_ref::<graphit_auth::profile_manager::BrokerRefreshError>()
            .is_some_and(
                |error| error.remote_effect() == graphit_auth::broker::oauth::RemoteEffect::NotSent
            )
    );
    assert!(failure.owner.is_none());
    assert!(!auth.paths.root().join("daemon/mcp.json").exists());
    assert!(!auth.paths.root().join("daemon/mcp.key").exists());
    loop {
        if subscriber.next().await.unwrap().instances().is_empty() {
            break;
        }
    }
    subscriber.shutdown().await.unwrap();
}

async fn foreground_inventory_owner_conflict(
    endpoint: (&str, &str),
    key: &Ed25519KeyPair,
    original: &RequestContext<'_>,
    connection: &mut PgConnection,
    capabilities: &[&str],
) {
    let (origin, raw) = endpoint;
    let auth = AuthFixture::new(
        origin,
        Credential::Native(raw.to_owned()),
        None,
        &ordinary(),
        original,
    );
    acl(connection, capabilities, &[auth.project_id()]).await;
    let claims = json!({"iss":origin,"sub":"another-verified-owner","aud":["broker-api"],"client_id":"graphit-cli",
        "jti":"foreground-owner-conflict","token_use":"access","scope":"presence-sdk","iat":seconds()-1,"exp":seconds()+3600});
    let message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(json!({"alg":"EdDSA","kid":"actual","typ":"JWT"}).to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let other = Zeroizing::new(format!(
        "{message}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(message.as_bytes()).as_ref())
    ));
    register_native(
        (origin, &other),
        auth.instance_id(),
        original,
        json!([{"project_id":auth.project_id(),"versions":["main"]}]),
    )
    .await;
    let auth = auth.release(original);
    let config = ConfigSnapshot::new(
        auth.paths.brand(),
        [ConfigDocument::new(
            auth.paths.brand(),
            ConfigSource::Command,
            [(
                ConfigKey::new("broker.allow_loopback_http").unwrap(),
                json!(true),
            )],
        )
        .unwrap()],
    )
    .unwrap();
    let inputs = [ProjectBootstrapInput {
        project: &auth.project_paths,
        explicit_version: "main",
        pin: None,
        detached_base: "",
    }];
    let mut failure = ForegroundSource::start_and_publish(
        &auth.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        original,
    )
    .await
    .err()
    .unwrap();
    let cause = failure
        .error
        .get_ref()
        .unwrap()
        .downcast_ref::<graphit_daemon::broker_inventory::OwnInventoryPublicationFailure>()
        .unwrap();
    assert!(matches!(
        cause,
        graphit_daemon::broker_inventory::OwnInventoryPublicationFailure::Remote {
            status: 409,
            code: graphit_daemon::broker_inventory::InventoryPublicationRemoteCode::OwnerConflict,
            ..
        }
    ));
    assert!(auth.paths.root().join("daemon/mcp.key").exists());
    failure.owner.take().unwrap().shutdown().await.unwrap();
    assert!(!auth.paths.root().join("daemon/mcp.json").exists());
    assert!(!auth.paths.root().join("daemon/mcp.key").exists());
}

async fn native_subscription<'op, 'auth>(
    client: &WebSocketClient,
    lease: &'op graphit_auth::ServiceCredentialLease<'auth>,
    original: &'op RequestContext<'auth>,
    config: NowSubscriptionConfig<'op>,
    service: &Cancellation,
    running: &mut tokio::task::JoinHandle<Result<(), ForegroundFailure>>,
) -> NowSubscription<'op, 'auth> {
    tokio::select! {
        subscription = start_now_subscription(client, lease, original, config) => match subscription {
            Ok(subscription) => subscription,
            Err(error) => local_operation_failure("Native subscription", error, service, running).await,
        },
        result = &mut *running => {
            eprintln!("Native owner ended before subscription: {result:?}");
            local_owner_failure("Native subscription", "awaiting admission", result).await
        }
    }
}

async fn native_snapshot(
    subscriber: &mut NowSubscription<'_, '_>,
    phase: &str,
    service: &Cancellation,
    running: &mut tokio::task::JoinHandle<Result<(), ForegroundFailure>>,
) -> RemoteNowSnapshot {
    tokio::select! {
        snapshot = subscriber.next() => match snapshot {
            Ok(snapshot) => snapshot,
            Err(error) => local_operation_failure(phase, error, service, running).await,
        },
        result = &mut *running => {
            eprintln!("Native owner ended before {phase}: {result:?}");
            local_owner_failure(phase, "awaiting native observation", result).await
        }
    }
}

async fn local_owner_failure(
    phase: &str,
    cause: impl std::fmt::Debug,
    mut result: Result<Result<(), ForegroundFailure>, tokio::task::JoinError>,
) -> ! {
    let cleanup = if let Ok(Err(failure)) = &mut result {
        if let Some(owner) = failure.owner.take() {
            Some(owner.shutdown().await)
        } else {
            None
        }
    } else {
        None
    };
    panic!("Local {phase}: operation={cause:?}; owner={result:?}; cleanup={cleanup:?}");
}

async fn local_operation_failure(
    phase: &str,
    cause: impl std::fmt::Debug,
    service: &Cancellation,
    running: &mut tokio::task::JoinHandle<Result<(), ForegroundFailure>>,
) -> ! {
    service.cancel();
    let joined = match tokio::time::timeout(Duration::from_secs(30), &mut *running).await {
        Ok(result) => result,
        Err(error) => {
            running.abort();
            let aborted = running.await;
            local_owner_failure(phase, (cause, error), aborted).await
        }
    };
    local_owner_failure(phase, cause, joined).await
}

async fn local_server(
    store: Arc<PostgresControlStore>,
    runtime: &RealtimeServerRuntime,
    connection: &mut PgConnection,
    realtime_config: RealtimeHttpConfig,
) -> Server {
    connection.execute("INSERT INTO public.local_users(username,subject,identity_kind,password_hash,name,email,organization,teams_json,enabled,password_change_required,revision,created_at,updated_at) VALUES('fixture','fixture-local','human','','Fixture','','','[]',1,0,1,0,0)").await.unwrap();
    let digest = pg::digester()
        .token(TokenKind::LocalAccess, LOCAL_RAW.as_bytes())
        .unwrap();
    sqlx_core::query::query("INSERT INTO public.local_tokens(token_hash,token_id,token_kind,subject,oidc_subject,principal_json,client_id,audience,resource,scopes_json,local_user_revision,family_id,auth_time,expires_at,created_at) VALUES($1,'runtime-local','access','fixture-local','local',$2,'graphit-cli','explicit-audience','','[\"presence-sdk\"]',1,'',0,$3,0)")
        .bind(digest.to_sql_hex()).bind(json!({"issuer":"local","subject":"fixture-local","username":"fixture","auth_method":"fixture","local_user_revision":1}).to_string())
        .bind(i64::try_from(seconds()*1_000_000).unwrap()+300_000_000).execute(&mut *connection).await.unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    Server::start(listener, local_routes(store, runtime, realtime_config))
}

fn local_routes(
    store: Arc<PostgresControlStore>,
    runtime: &RealtimeServerRuntime,
    realtime_config: RealtimeHttpConfig,
) -> axum::Router {
    local_realtime_router(
        store.clone(),
        LocalRealtimeHttpConfig {
            audience: "explicit-audience".into(),
            identity_json_bytes: n(16384),
            realtime: realtime_config,
        },
        runtime,
    )
    .unwrap()
    .merge(
        instance_inventory_router(
            store.clone(),
            InstanceRegistryHttpConfig {
                audience: "explicit-audience".into(),
                required_scopes: vec!["presence-sdk".into()],
                identity_json_bytes: n(16384),
                grants_json_bytes: n(65536),
                body_bytes: n(65536),
                request_timeout: Duration::from_secs(2),
            },
            InstanceInventoryReadHttpConfig {
                audience: "explicit-audience".into(),
                required_scopes: vec!["presence-sdk".into()],
                identity_json_bytes: n(16384),
                grants_json_bytes: n(65536),
                inventory_json_bytes: n(65536),
                request_timeout: Duration::from_secs(2),
            },
            InstanceInventoryListHttpConfig {
                audience: "explicit-audience".into(),
                required_scopes: vec!["presence-sdk".into()],
                identity_json_bytes: n(16384),
                grants_json_bytes: n(65536),
                inventory_json_bytes: n(65536),
                request_timeout: Duration::from_secs(2),
                query_bytes: n(4096),
            },
        )
        .unwrap(),
    )
}

async fn foreground_local_owner(
    store: Arc<PostgresControlStore>,
    runtime: &RealtimeServerRuntime,
    connection: &mut PgConnection,
    original: &RequestContext<'_>,
    capabilities: &[&str],
) {
    let server = local_server(store.clone(), runtime, connection, realtime()).await;
    let auth = AuthFixture::new(
        &server.origin,
        Credential::Local,
        None,
        &ordinary(),
        original,
    );
    let observer = AuthFixture::new(
        &server.origin,
        Credential::Local,
        None,
        &ordinary(),
        original,
    );
    acl(
        connection,
        capabilities,
        &[auth.project_id(), observer.project_id()],
    )
    .await;
    let auth = auth.release(original);
    let config = ConfigSnapshot::new(
        auth.paths.brand(),
        [ConfigDocument::new(
            auth.paths.brand(),
            ConfigSource::Command,
            [(
                ConfigKey::new("broker.allow_loopback_http").unwrap(),
                json!(true),
            )],
        )
        .unwrap()],
    )
    .unwrap();
    let inputs = [ProjectBootstrapInput {
        project: &auth.project_paths,
        explicit_version: "branch/ação",
        pin: None,
        detached_base: "",
    }];
    let owner = ForegroundSource::start_and_publish(
        &auth.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        original,
    )
    .await
    .unwrap();
    let address = owner.address().unwrap();
    let verifier = store
        .local_bearer_verifier("explicit-audience", n(16384))
        .unwrap();
    let scopes = realtime().required_scopes;
    let verification = verifier
        .verify(
            LOCAL_RAW,
            &scopes,
            Instant::now() + realtime().operation_timeout,
        )
        .await;
    assert!(
        verification.is_ok(),
        "Local verifier before Source: {:?}",
        verification.err()
    );
    drop(verification);
    let service = Cancellation::default();
    let operational = service.clone();
    let mut running = tokio::spawn(async move { owner.run(&operational).await });
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let status = tokio::select! {
        biased;
        result = &mut running => local_owner_failure("local HTTP startup", "Source exited", result).await,
        result = graphit_app::daemon_control::request(
            &auth.paths, address, &client, native_limits(), original,
            graphit_app::daemon_control::Operation::Status, None,
        ) => match result {
            Ok(status) => status,
            Err(error) => local_operation_failure("local HTTP startup", error, &service, &mut running).await,
        },
    };
    assert_eq!(status.state, graphit_app::daemon_control::State::Running);
    assert_eq!(status.pid, std::process::id());
    let caller = observer
        .auth
        .issuer()
        .authenticate_runtime(&observer.runtime_key(original), original)
        .unwrap();
    let mut selection = observer
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "now")
        .unwrap();
    let lease = selection.credential().unwrap();
    let transport = websocket(&server.origin);
    let subscription = start_now_subscription(
        &transport,
        &lease,
        original,
        NowSubscriptionConfig {
            project_id: auth.project_id(),
            instance_id: Some(auth.instance_id()),
            version: None,
            command_bytes: n(65536),
            snapshot_bytes: n(65536),
        },
    );
    let mut subscriber = tokio::select! {
        biased;
        result = &mut running => local_owner_failure("observer upgrade", "Source exited", result).await,
        result = subscription => match result {
            Ok(subscriber) => subscriber,
            Err(error) => local_operation_failure("observer upgrade", error, &service, &mut running).await,
        },
    };
    let verification = tokio::select! {
        biased;
        result = &mut running => local_owner_failure("active verifier", "Source exited", result).await,
        result = verifier.verify(LOCAL_RAW, &scopes, Instant::now() + realtime().operation_timeout) => match result {
            Ok(verified) => verified,
            Err(error) => local_operation_failure("active verifier", error, &service, &mut running).await,
        },
    };
    drop(verification);
    let first_observation = loop {
        let snapshot = tokio::select! {
            biased;
            result = &mut running => local_owner_failure("first snapshot", "Source exited", result).await,
            result = subscriber.next() => match result {
                Ok(snapshot) => snapshot,
                Err(error) => local_operation_failure("first snapshot", error, &service, &mut running).await,
            },
        };
        if let Some(instance) = snapshot.instances().first() {
            assert_eq!(instance.version(), "branch/ação");
            let now = serde_json::to_value(instance.now()).unwrap();
            for section in ["tasks", "sessions", "memories", "knowledge"] {
                assert_eq!(now["sections"][section]["availability"], "no_corpus");
                assert_eq!(now["sections"][section]["total"], 0);
                assert_eq!(now["sections"][section]["has_more"], false);
                assert!(now["sections"][section]["error"].is_null());
                assert!(
                    now["sections"][section]["items"]
                        .as_array()
                        .unwrap()
                        .is_empty()
                );
            }
            break now["generated_at"].clone();
        }
    };
    // Wait for a genuinely recomposed observation through the real negotiated
    // heartbeat, rather than accepting a repeated publication of the first read.
    loop {
        let snapshot = native_snapshot(
            &mut subscriber,
            "local next heartbeat",
            &service,
            &mut running,
        )
        .await;
        if let Some(instance) = snapshot.instances().first() {
            assert_eq!(instance.version(), "branch/ação");
            let now = serde_json::to_value(instance.now()).unwrap();
            if now["generated_at"] != first_observation {
                break;
            }
        }
    }
    let status = tokio::select! {
        biased;
        result = &mut running => local_owner_failure("local HTTP after heartbeat", "Source exited", result).await,
        result = graphit_app::daemon_control::request(
            &auth.paths, address, &client, native_limits(), original,
            graphit_app::daemon_control::Operation::Status, None,
        ) => match result {
            Ok(status) => status,
            Err(error) => local_operation_failure("local HTTP after heartbeat", error, &service, &mut running).await,
        },
    };
    assert_eq!(status.state, graphit_app::daemon_control::State::Running);
    assert_eq!(status.pid, std::process::id());
    service.cancel();
    tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    loop {
        if subscriber.next().await.unwrap().instances().is_empty() {
            break;
        }
    }
    assert!(!auth.paths.root().join("daemon/mcp.json").exists());
    assert!(!auth.paths.root().join("daemon/mcp.key").exists());
    subscriber.shutdown().await.unwrap();
    server.shutdown().await;
}

struct NativeBroker {
    fixture: pg::Fixture,
    connection: PgConnection,
    store: Arc<PostgresControlStore>,
    key: Arc<Ed25519KeyPair>,
    origin: String,
    raw: Zeroizing<String>,
    server: Server,
    tokens: Arc<RefreshEndpoint>,
    _runtime: RealtimeServerRuntime,
}
async fn native_broker(realtime_config: RealtimeHttpConfig) -> NativeBroker {
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
    let routes = broker_realtime_router(store.clone(), verifier.clone(), realtime_config, &runtime)
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
    let tokens = Arc::new(RefreshEndpoint {
        origin: origin.clone(),
        key: Arc::clone(&key),
        calls: std::sync::atomic::AtomicUsize::new(0),
        mode: std::sync::atomic::AtomicU8::new(0),
        response_at: std::sync::atomic::AtomicU64::new(0),
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let endpoint = Arc::clone(&tokens);
    let routes = routes.route(
        "/token",
        axum::routing::post(move |body: axum::body::Bytes| {
            let endpoint = Arc::clone(&endpoint);
            async move { endpoint.respond(body).await }
        }),
    );
    let server = Server::start(listener, routes);
    NativeBroker {
        fixture,
        connection,
        store,
        key,
        origin,
        raw,
        server,
        tokens,
        _runtime: runtime,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real Foreground/Auth/Core Lance/Broker TCP WS/PostgreSQL TLS NOSUPERUSER"]
async fn native_foreground_now_two_projects_and_owned_shutdown() {
    let NativeBroker {
        fixture,
        mut connection,
        store,
        key,
        origin,
        raw,
        server: native_server,
        tokens: _,
        _runtime,
    } = native_broker(RealtimeHttpConfig {
        operation_timeout: Duration::from_secs(30),
        ..realtime()
    })
    .await;
    let capabilities = [
        "presence:read",
        "presence:publish",
        "task:read",
        "memory:read",
        "knowledge:read",
    ];
    let cancellation = Cancellation::default();
    let original = request(&cancellation, Duration::from_secs(600));
    let first = AuthFixture::new(
        &origin,
        Credential::Native(raw.to_string()),
        None,
        &ordinary(),
        &original,
    );
    let second = AuthFixture::new(&origin, Credential::Local, None, &ordinary(), &original);
    let writer = AuthFixture::new(
        &origin,
        Credential::Native(raw.to_string()),
        Some(first.project_paths.root()),
        &ordinary(),
        &original,
    );
    // The local writer seeds the same real PROJECT corpora. The retained
    // publisher and observer remain Native Broker consumers; their Source
    // composes the actual RecordStorage resolver and validates discovery.
    let records = AuthFixture::new(
        &origin,
        Credential::Local,
        Some(first.project_paths.root()),
        &ordinary(),
        &original,
    );
    let seeded_heads = seed_foreground_projects(&records, &second, &original).await;
    let first = foreground_channels_and_lifecycle(
        (first, seeded_heads),
        &second,
        &writer,
        &records,
        &original,
        &mut connection,
        &capabilities,
        (&origin, &raw),
    )
    .await;
    foreground_signed_expiry_and_no_corpus(
        &origin,
        &key,
        &writer,
        &original,
        &mut connection,
        &capabilities,
        &[first.project_id(), second.project_id()],
    )
    .await;
    foreground_inventory_owner_conflict(
        (&origin, &raw),
        &key,
        &original,
        &mut connection,
        &capabilities,
    )
    .await;
    native_server.shutdown().await;
    connection.close().await.unwrap();
    store.shutdown(pg::deadline()).await.unwrap();
    drop(store);
    fixture.shutdown().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real Local Foreground/Auth/Core/Broker TCP WS/PostgreSQL TLS NOSUPERUSER"]
async fn native_foreground_now_local_owned_shutdown() {
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

    let runtime = RealtimeServerRuntime::new(Duration::from_secs(4), n(8)).unwrap();
    let cancellation = Cancellation::default();
    let original = request(&cancellation, Duration::from_secs(600));
    foreground_local_owner(
        store.clone(),
        &runtime,
        &mut connection,
        &original,
        &capabilities,
    )
    .await;
    connection.close().await.unwrap();
    store.shutdown(pg::deadline()).await.unwrap();
    drop(store);
    fixture.shutdown().unwrap();
}

fn install_memory_model(
    installation: &InstallationPaths,
    original: &RequestContext<'_>,
) -> (
    model_api::ExpectedModel,
    Vec<(std::ffi::OsString, std::ffi::OsString)>,
    model_fixture::Fixture,
) {
    let fixture = model_fixture::Fixture::actual_with_original(
        model_api::TaskKind::Embedding,
        original.snapshot(),
    );
    let settings = fixture.settings(&[graphit_app::models::DEFAULT_SLOT]);
    ConfigStore::global(installation)
        .unwrap()
        .set(
            ConfigKey::new("daemon.models").unwrap(),
            serde_json::to_value(&settings).unwrap(),
            Duration::from_secs(5),
            original.cancellation(),
        )
        .unwrap();
    let options = graphit_daemon::model_child::ChildOptions::standard().unwrap();
    let inputs = settings
        .slots
        .iter()
        .map(|slot| {
            model_api::CapturedModelInput::capture(
                &slot.name,
                &slot.bundle,
                &options.capture,
                options.source,
                original.snapshot().clone(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let config = model_api::ResolvedConfig::resolve(
        settings,
        model_fixture::cpu_selection(),
        inputs,
        original.snapshot(),
        &options.manifest,
    )
    .unwrap();
    let expected =
        model_api::ExpectedModel::new(&config, graphit_app::models::DEFAULT_SLOT, "now").unwrap();
    let executable = std::path::PathBuf::from(
        std::env::var_os("GRAPHIT_TEST_MODEL_SERVER").expect("qualified compiled model server"),
    );
    assert!(executable.is_absolute() && executable.is_file());
    let environment = vec![(
        "PATH".into(),
        executable.parent().unwrap().as_os_str().to_owned(),
    )];
    (expected, environment, fixture)
}

async fn published_memory_generation(
    fixture: &AuthFixture,
    memory: &graphit_records::memory_store::Memory,
    expected: &model_api::ExpectedModel,
    original: &RequestContext<'_>,
) -> std::io::Result<()> {
    let caller = fixture
        .auth
        .issuer()
        .authenticate_runtime(&fixture.runtime_key(original), original)
        .map_err(std::io::Error::other)?;
    let authority = ProjectRecordAuthority::new(fixture.auth.issuer());
    let read = authority
        .admit_authenticated_read(
            &caller,
            original,
            fixture.scope(&fixture.version),
            ProjectReadModule::Memory,
        )
        .map_err(std::io::Error::other)?;
    let metadata = graphit_app::embedding_service::EmbeddingMetadata::local(expected)
        .map_err(std::io::Error::other)?;
    let identity =
        graphit_app::foreground_memory_jobs::MemoryJobResources::memory_identity(&metadata)
            .map_err(std::io::Error::other)?;
    // A valid query vector is only search input; index publication comes from the real M job.
    let query = vec![1.0; usize::try_from(expected.dimensions().unwrap()).unwrap()];
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let result = NativeMemoryStore::search_semantic(
                original.snapshot(),
                &read,
                identity.clone(),
                &query,
                graphit_records::memory_store::MemorySemanticSearchOptions {
                    filter: graphit_records::memory_store::MemoryListFilter::default(),
                    include_deleted_history: false,
                    top_k: n(8),
                },
                memory_write().read,
            )
            .await
            .map_err(std::io::Error::other)?;
            if let graphit_records::memory_store::MemorySemanticSearchRead::Published {
                generation,
                hits,
                ..
            } = result
                && let Some(hit) = hits.iter().find(|hit| hit.current.id == memory.id)
            {
                assert_eq!(generation.project_id, fixture.project_id());
                assert_eq!(generation.project_version, fixture.version.version());
                assert_eq!(hit.current.revision, memory.revision);
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(std::io::Error::other)?
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real combined Now/M child GGUF CPU, SDK semantic generations and PostgreSQL TLS"]
async fn native_foreground_now_with_active_memory_isolates_publishers_until_stop() {
    foreground_now_with_active_memory(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real CLI administrative stop with owned GGUF CPU child and PostgreSQL TLS"]
async fn native_foreground_now_with_active_memory_admin_stop_joins() {
    foreground_now_with_active_memory(true).await;
}

async fn foreground_now_with_active_memory(admin_stop: bool) {
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

    let runtime = RealtimeServerRuntime::new(Duration::from_secs(4), n(8)).unwrap();
    let server = local_server(
        store.clone(),
        &runtime,
        &mut connection,
        RealtimeHttpConfig {
            operation_timeout: Duration::from_secs(30),
            ..realtime()
        },
    )
    .await;
    let cancellation = Cancellation::default();
    let original = request(&cancellation, Duration::from_secs(600));
    let embedding = AiService::Local(LocalExecution {
        device: LocalDevice::Cpu,
        device_id: 0,
    });
    let first = AuthFixture::with_embedding(
        &server.origin,
        Credential::Local,
        None,
        &ordinary(),
        embedding.clone(),
        &original,
    );
    let second = AuthFixture::with_embedding(
        &server.origin,
        Credential::Local,
        None,
        &ordinary(),
        embedding.clone(),
        &original,
    );
    let observer = AuthFixture::with_embedding(
        &server.origin,
        Credential::Local,
        Some(first.project_paths.root()),
        &ordinary(),
        embedding,
        &original,
    );
    let ids = [first.project_id(), second.project_id()];
    acl(&mut connection, &capabilities, &ids).await;
    let first_caller = observer
        .auth
        .issuer()
        .authenticate_runtime(&observer.runtime_key(&original), &original)
        .unwrap();
    let first_memory = add_memory(
        &observer.auth,
        &first_caller,
        &original,
        observer.scope(&observer.version),
        "active-first",
    )
    .await;
    let second_caller = second
        .auth
        .issuer()
        .authenticate_runtime(&second.runtime_key(&original), &original)
        .unwrap();
    let second_memory = add_memory(
        &second.auth,
        &second_caller,
        &original,
        second.scope(&second.version),
        "active-second",
    )
    .await;
    drop(first_caller);
    drop(second_caller);
    let (expected, environment, _model_fixture) = install_memory_model(&first.paths, &original);
    let first = first.release(&original);
    let config = ConfigSnapshot::new(
        first.paths.brand(),
        [ConfigDocument::new(
            first.paths.brand(),
            ConfigSource::Command,
            [(
                ConfigKey::new("broker.allow_loopback_http").unwrap(),
                json!(true),
            )],
        )
        .unwrap()],
    )
    .unwrap();
    let inputs = [
        ProjectBootstrapInput {
            project: &first.project_paths,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        },
        ProjectBootstrapInput {
            project: &second.project_paths,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        },
    ];
    let verifier = store
        .local_bearer_verifier("explicit-audience", n(16384))
        .unwrap();
    let scopes = realtime().required_scopes;
    let before = Instant::now();
    let verified = verifier
        .verify(LOCAL_RAW, &scopes, before + realtime().operation_timeout)
        .await;
    eprintln!(
        "combined startup Local verifier: elapsed_ms={}; error={:?}",
        before.elapsed().as_millis(),
        verified.as_ref().err()
    );
    assert!(verified.is_ok(), "Local verifier baseline before Source");
    drop(verified);
    let service = Cancellation::default();
    let operational = service.clone();
    let mut administrative = None;
    let mut local_address = None;
    let mut running = if admin_stop {
        let inventory = GlobalProjectInventory::at(&first.paths, 65536).unwrap();
        for input in &inputs {
            let private = installation_bootstrap::prepare(
                &first.paths,
                BootstrapLimits::new(65536, 256).unwrap(),
                original.snapshot(),
            )
            .into_installation()
            .unwrap();
            bootstrap_project(
                ProjectBootstrapInput {
                    project: input.project,
                    explicit_version: input.explicit_version,
                    pin: None,
                    detached_base: "",
                },
                ProjectBootstrapLimits {
                    scope: ScopeLimits::new(16, 4096, 128).unwrap(),
                    native: native_limits(),
                },
                &inventory,
                private,
                original.snapshot(),
            )
            .unwrap();
        }
        let stderr = first.paths.root().join("admin-stop.log");
        let mut daemon = tokio::process::Command::new(env!("CARGO_BIN_EXE_graphit"))
            .current_dir(first.project_paths.root())
            .env_clear()
            .envs(environment)
            .env("HOME", first._directory.path())
            .arg("--global-dir")
            .arg(first.paths.root())
            .arg("-c")
            .arg(format!("agent={}", json!(NativeBinding::KiroV3)))
            .args([
                "-c",
                "broker.allow_loopback_http=true",
                "daemon",
                "foreground",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::fs::File::create(&stderr).unwrap())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let pid = std::num::NonZeroU32::new(daemon.id().unwrap()).unwrap();
        let until = Instant::now() + Duration::from_secs(60);
        let record = loop {
            assert!(
                daemon.try_wait().unwrap().is_none(),
                "daemon exited before discovery: {}",
                std::fs::read_to_string(&stderr).unwrap()
            );
            if let Some(record) = graphit_daemon::source_discovery::read_discovery(
                &first.paths,
                native_limits(),
                original.snapshot(),
            )
            .unwrap()
            {
                break record;
            }
            assert!(Instant::now() < until, "daemon discovery deadline");
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        administrative = Some((record.address(), pid));
        tokio::spawn(async move {
            let status = tokio::select! {
                status = daemon.wait() => status.unwrap(),
                () = operational.cancelled() => {
                    daemon.kill().await.unwrap();
                    return Err(ForegroundFailure::from(std::io::Error::from(std::io::ErrorKind::Interrupted)));
                }
            };
            assert_eq!(
                status.code(),
                Some(0),
                "daemon administrative stop: {status}; {}",
                std::fs::read_to_string(stderr).unwrap()
            );
            eprintln!("combined administrative stop: daemon_exit_code=0");
            Ok(())
        })
    } else {
        let owner = match ForegroundSource::start_and_publish(
            &first.paths,
            &inputs,
            0,
            &config,
            NativeBinding::KiroV3,
            environment,
            &original,
        )
        .await
        {
            Ok(owner) => owner,
            Err(failure) => {
                local_owner_failure(
                    "combined startup",
                    "Inventory publication before M startup",
                    Ok(Err(failure)),
                )
                .await
            }
        };
        local_address = owner.address();
        tokio::spawn(async move { owner.run(&operational).await })
    };
    let caller = observer
        .auth
        .issuer()
        .authenticate_runtime(&observer.runtime_key(&original), &original)
        .unwrap();
    let mut selection = observer
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "now")
        .unwrap();
    let lease = selection.credential().unwrap();
    let transport = websocket(&server.origin);
    let mut subscribers = Vec::new();
    for project_id in ids {
        let subscription = start_now_subscription(
            &transport,
            &lease,
            &original,
            NowSubscriptionConfig {
                project_id,
                instance_id: Some(first.instance_id()),
                version: Some("main"),
                command_bytes: n(65536),
                snapshot_bytes: n(65536),
            },
        );
        let mut subscriber = tokio::select! {
            biased;
            result = &mut running => local_owner_failure("combined upgrade", "Source exited", result).await,
            result = subscription => match result {
                Ok(subscriber) => subscriber,
                Err(error) => local_operation_failure("combined upgrade", error, &service, &mut running).await,
            },
        };
        loop {
            let snapshot = tokio::select! {
                biased;
                result = &mut running => local_owner_failure("combined snapshot", "Source exited", result).await,
                result = subscriber.next() => match result {
                    Ok(snapshot) => snapshot,
                    Err(error) => local_operation_failure("combined snapshot", error, &service, &mut running).await,
                },
            };
            if let Some(instance) = snapshot.instances().first() {
                assert_eq!(instance.version(), "main");
                let now = serde_json::to_value(instance.now()).unwrap();
                for section in ["tasks", "sessions"] {
                    assert_eq!(now["sections"][section]["availability"], "no_corpus");
                    assert_eq!(now["sections"][section]["total"], 0);
                    assert_eq!(now["sections"][section]["has_more"], false);
                    assert!(now["sections"][section]["error"].is_null());
                    assert!(
                        now["sections"][section]["items"]
                            .as_array()
                            .unwrap()
                            .is_empty()
                    );
                }
                break;
            }
        }
        subscribers.push(subscriber);
    }
    for (fixture, memory) in [(&observer, &first_memory), (&second, &second_memory)] {
        tokio::select! {
            biased;
            result = &mut running => local_owner_failure("combined Memory publication", "Source exited", result).await,
            result = published_memory_generation(fixture, memory, &expected, &original) => {
                if let Err(error) = result {
                    local_operation_failure("combined Memory publication", error, &service, &mut running).await;
                }
            },
        }
    }
    if let Some((address, pid)) = administrative {
        use graphit_app::daemon_control::{self, Operation, State};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let executable =
            std::fs::canonicalize(std::env::var_os("GRAPHIT_TEST_MODEL_SERVER").unwrap()).unwrap();
        let mut children = std::collections::BTreeSet::new();
        for thread in std::fs::read_dir(format!("/proc/{pid}/task")).unwrap() {
            let path = thread.unwrap().path().join("children");
            let child_pids = match std::fs::read_to_string(path) {
                Ok(pids) => pids,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => panic!("owned Source thread children: {error}"),
            };
            for child in child_pids.split_whitespace() {
                let child: u32 = child.parse().unwrap();
                if std::fs::read_link(format!("/proc/{child}/exe")).is_ok_and(|p| p == executable) {
                    children.insert(child);
                }
            }
        }
        assert_eq!(children.len(), 1, "Source must own one actual model child");
        let model_pid = *children.first().unwrap();
        let mut partial = tokio::net::TcpStream::connect(address).await.unwrap();
        partial
            .write_all(b"POST /mcp HTTP/1.1\r\nHost: localhost\r\n")
            .await
            .unwrap();
        let mut byte = [0];
        assert!(
            tokio::time::timeout(Duration::from_millis(25), partial.read(&mut byte))
                .await
                .is_err(),
            "partial HTTP connection must remain open before stop"
        );
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let headers =
            graphit_app::ast_source::runtime_headers(&first.paths, native_limits(), &original)
                .await
                .unwrap();
        let mut native_now = tokio::time::timeout(
            Duration::from_secs(30),
            client
                .post(format!("http://{address}/api/now/subscribe"))
                .headers(headers.into_iter().collect())
                .json(&json!({
                    "project_id": ids[0],
                    "provider": "now",
                    "version": "main",
                    "instance_id": first.instance_id(),
                }))
                .send(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(native_now.status(), reqwest::StatusCode::OK);
        assert_eq!(native_now.headers()["content-type"], "application/x-ndjson");
        let first_frame = tokio::time::timeout(Duration::from_secs(30), async {
            let mut bytes = Vec::new();
            loop {
                let chunk = native_now.chunk().await.unwrap().unwrap();
                assert!(bytes.len() + chunk.len() <= 65536);
                bytes.extend_from_slice(&chunk);
                if let Some(end) = bytes.iter().position(|byte| *byte == b'\n') {
                    break serde_json::from_slice::<serde_json::Value>(&bytes[..end]).unwrap();
                }
            }
        })
        .await
        .unwrap();
        assert!(!first_frame["instances"].as_array().unwrap().is_empty());
        let reply = daemon_control::request(
            &first.paths,
            address,
            &client,
            native_limits(),
            &original,
            Operation::Stop,
            Some(pid),
        )
        .await
        .unwrap();
        assert_eq!(reply.pid, pid.get());
        assert_eq!(reply.state, State::Stopping);
        tokio::time::timeout(Duration::from_secs(30), &mut running)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{model_pid}")).exists());
        assert!(tokio::net::TcpStream::connect(address).await.is_err());
        drop(native_now);
        match tokio::time::timeout(Duration::from_secs(2), partial.read(&mut byte))
            .await
            .unwrap()
        {
            Ok(0) => (),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ) => {}
            result => panic!("partial HTTP connection survived stop: {result:?}"),
        }
        eprintln!(
            "combined administrative stop: owned_model_reaped=true; active_native_now_joined=true; partial_http_closed=true"
        );
    } else {
        acl(
            &mut connection,
            &[
                "presence:read",
                "task:read",
                "memory:read",
                "knowledge:read",
            ],
            &ids,
        )
        .await;
        for subscriber in &mut subscribers {
            while !native_snapshot(
                subscriber,
                "publisher denied with active Memory",
                &service,
                &mut running,
            )
            .await
            .instances()
            .is_empty()
            {}
        }
        assert!(
            !running.is_finished(),
            "Now denial must not stop Memory or local Source"
        );
        assert_local_source_operational(
            &first.paths,
            local_address.unwrap(),
            &first.project_paths,
            &original,
        )
        .await;
        let fresh = add_memory(
            &observer.auth,
            &caller,
            &original,
            observer.scope(&observer.version),
            "memory-after-now-denial",
        )
        .await;
        // Native Store writes do not dispatch Source work. The authenticated
        // host Stop boundary requests the real background Memory rescan.
        let hook_policy = graphit_hub::policy::HubResourcePolicy::resolve(&config).unwrap();
        let hook = graphit_app::session_hook_entry::prepare(
            &first.paths,
            &graphit_app::session_hook_entry::Arguments {
                recipient: graphit_app::session_hook_entry::Recipient::Coordinator,
                engine: graphit_app::session_hook_entry::Engine::Kiro,
                event: graphit_app::session_hook_entry::Event::Stop,
                project_dir: Some(first.project_paths.root().to_owned()),
                task_session: None,
                task_ids: Vec::new(),
            },
            Some("main"),
            &original,
        )
        .unwrap();
        let mut hook_output = Vec::new();
        graphit_app::session_hook_entry::call(
            &first.paths,
            graphit_app::session_hook_entry::Recipient::Coordinator,
            hook,
            native_limits(),
            &hook_policy,
            &original,
            &mut hook_output,
        )
        .await
        .unwrap();
        eprintln!("post-denial authenticated Kiro Stop requested Memory rescan");
        published_memory_generation(&observer, &fresh, &expected, &original)
            .await
            .unwrap();
        service.cancel();
        tokio::time::timeout(Duration::from_secs(30), &mut running)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    assert!(!first.paths.root().join("daemon/mcp.json").exists());
    assert!(!first.paths.root().join("daemon/mcp.key").exists());
    for mut subscriber in subscribers {
        loop {
            if subscriber.next().await.unwrap().instances().is_empty() {
                break;
            }
        }
        subscriber.shutdown().await.unwrap();
    }
    server.shutdown().await;
    connection.close().await.unwrap();
    store.shutdown(pg::deadline()).await.unwrap();
    drop(store);
    fixture.shutdown().unwrap();
}

#[path = "foreground_now/live_inventory.rs"]
mod live_inventory;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real Native Foreground/Auth/Core and Broker WS/PostgreSQL TLS NOSUPERUSER"]
async fn native_foreground_now_recovers_disconnect_before_first_welcome() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let NativeBroker {
        fixture,
        mut connection,
        store,
        origin,
        raw,
        server,
        _runtime,
        ..
    } = native_broker(realtime()).await;
    let cancellation = Cancellation::default();
    let original = request(&cancellation, Duration::from_secs(600));
    let auth = AuthFixture::new(
        &origin,
        Credential::Native(raw.to_string()),
        None,
        &ordinary(),
        &original,
    );
    let observer = AuthFixture::new(
        &origin,
        Credential::Native(raw.to_string()),
        None,
        &ordinary(),
        &original,
    );
    acl(
        &mut connection,
        &[
            "presence:publish",
            "presence:read",
            "task:read",
            "memory:read",
            "knowledge:read",
        ],
        &[auth.project_id(), observer.project_id()],
    )
    .await;
    let auth = auth.release(&original);
    let config = ConfigSnapshot::new(
        auth.paths.brand(),
        [ConfigDocument::new(
            auth.paths.brand(),
            ConfigSource::Command,
            [(
                ConfigKey::new("broker.allow_loopback_http").unwrap(),
                json!(true),
            )],
        )
        .unwrap()],
    )
    .unwrap();
    let inputs = [ProjectBootstrapInput {
        project: &auth.project_paths,
        explicit_version: "main",
        pin: None,
        detached_base: "",
    }];
    let owner = ForegroundSource::start_and_publish(
        &auth.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        &original,
    )
    .await
    .unwrap();
    let address = owner.address().unwrap();
    // Startup has published inventory and loaded the genuine Native verifier.
    // Close actual upgrade connections before any publisher can receive welcome.
    let routes = server.app.clone();
    server.shutdown().await;
    let listener = TcpListener::bind(origin.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let disconnects = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(socket.read_u8().await.unwrap());
            }
            assert!(head.starts_with(b"GET /v1/realtime "));
            socket.shutdown().await.unwrap();
        }
        listener
    });
    let service = Cancellation::default();
    let operational = service.clone();
    let mut running = tokio::spawn(async move { owner.run(&operational).await });
    let listener = tokio::select! {
        result = disconnects => result.unwrap(),
        result = &mut running => panic!("Source ended before first-welcome readmission: {result:?}"),
        () = tokio::time::sleep(Duration::from_secs(15)) => panic!("Native publisher did not readmit its upgrade"),
    };
    assert!(!running.is_finished());
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let status = graphit_app::daemon_control::request(
        &auth.paths,
        address,
        &client,
        native_limits(),
        &original,
        graphit_app::daemon_control::Operation::Status,
        None,
    )
    .await
    .unwrap();
    assert_eq!(status.state, graphit_app::daemon_control::State::Running);
    // Restore the exact real Broker router/runtime; no fabricated welcome or heads.
    let server = Server::start(listener, routes);
    let caller = observer
        .auth
        .issuer()
        .authenticate_runtime(&observer.runtime_key(&original), &original)
        .unwrap();
    let mut selection = observer
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "now")
        .unwrap();
    let cache = cache();
    let verifier = cache.get(&mut selection).unwrap();
    let lease = selection.native_credential(&verifier).await.unwrap();
    let transport = websocket(&origin);
    let mut subscriber = start_now_subscription(
        &transport,
        &lease,
        &original,
        NowSubscriptionConfig {
            project_id: auth.project_id(),
            instance_id: Some(auth.instance_id()),
            version: None,
            command_bytes: n(65536),
            snapshot_bytes: n(65536),
        },
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let snapshot = subscriber.next().await.unwrap();
            if let Some(instance) = snapshot.instances().first() {
                assert_eq!(instance.instance_id(), auth.instance_id());
                assert_eq!(instance.version(), "main");
                break;
            }
        }
    })
    .await
    .expect("real Native welcome and fresh snapshot after reconnect");
    service.cancel();
    tokio::time::timeout(Duration::from_secs(15), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    subscriber.shutdown().await.unwrap();
    assert!(!auth.paths.root().join("daemon/mcp.json").exists());
    server.shutdown().await;
    _runtime.shutdown().await.unwrap();
    drop(observer);
    connection.close().await.unwrap();
    store.shutdown(pg::deadline()).await.unwrap();
    drop(store);
    fixture.shutdown().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real Foreground/Auth/Core/Memory and Broker WS/PostgreSQL TLS NOSUPERUSER"]
async fn foreground_now_recovers_broker_loss_with_fresh_state_and_cancelable_wait() {
    let fixture = pg::Fixture::new(true).await;
    let mut connection = fixture.connection().await;
    connection.execute("CREATE ROLE runtime_now_owner LOGIN NOSUPERUSER PASSWORD 'ephemeral-owner'; DROP SCHEMA public CASCADE; CREATE SCHEMA public AUTHORIZATION runtime_now_owner; GRANT CONNECT,TEMP ON DATABASE postgres TO runtime_now_owner").await.unwrap();
    let store = Arc::new(
        PostgresControlStore::open(pg_config(&fixture), pg::digester(), pg::deadline())
            .await
            .unwrap(),
    );
    let runtime = RealtimeServerRuntime::new(Duration::from_secs(4), n(8)).unwrap();
    let server = local_server(store.clone(), &runtime, &mut connection, realtime()).await;
    let origin = server.origin.clone();
    let broker_address = origin.strip_prefix("http://").unwrap().to_owned();
    let cancellation = Cancellation::default();
    let original = request(&cancellation, Duration::from_secs(600));
    let auth = AuthFixture::new(&origin, Credential::Local, None, &ordinary(), &original);
    let writer = AuthFixture::new(
        &origin,
        Credential::Local,
        Some(auth.project_paths.root()),
        &ordinary(),
        &original,
    );
    let observer = AuthFixture::new(&origin, Credential::Local, None, &ordinary(), &original);
    let ids = [auth.project_id(), observer.project_id()];
    let capabilities = [
        "presence:read",
        "presence:publish",
        "task:read",
        "memory:read",
        "knowledge:read",
    ];
    acl(&mut connection, &capabilities, &ids).await;
    let writer_caller = writer
        .auth
        .issuer()
        .authenticate_runtime(&writer.runtime_key(&original), &original)
        .unwrap();
    add_memory(
        &writer.auth,
        &writer_caller,
        &original,
        writer.scope(&writer.version),
        "before-disconnect",
    )
    .await;
    let auth = auth.release(&original);
    let config = ConfigSnapshot::new(
        auth.paths.brand(),
        [ConfigDocument::new(
            auth.paths.brand(),
            ConfigSource::Command,
            [(
                ConfigKey::new("broker.allow_loopback_http").unwrap(),
                json!(true),
            )],
        )
        .unwrap()],
    )
    .unwrap();
    let inputs = [ProjectBootstrapInput {
        project: &auth.project_paths,
        explicit_version: "main",
        pin: None,
        detached_base: "",
    }];
    let owner = ForegroundSource::start_and_publish(
        &auth.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        &original,
    )
    .await
    .unwrap();
    let source_address = owner.address().unwrap();
    let service = Cancellation::default();
    let operational = service.clone();
    let mut running = tokio::spawn(async move { owner.run(&operational).await });
    let observer_caller = observer
        .auth
        .issuer()
        .authenticate_runtime(&observer.runtime_key(&original), &original)
        .unwrap();
    let mut selection = observer
        .auth
        .issuer()
        .capture_presence_read_service(&observer_caller, "now")
        .unwrap();
    let lease = selection.credential().unwrap();
    let transport = websocket(&origin);
    let subscription = NowSubscriptionConfig {
        project_id: auth.project_id(),
        instance_id: Some(auth.instance_id()),
        version: None,
        command_bytes: n(65536),
        snapshot_bytes: n(65536),
    };
    let mut subscriber = start_now_subscription(&transport, &lease, &original, subscription)
        .await
        .unwrap();
    let generated = loop {
        let snapshot = subscriber.next().await.unwrap();
        if let Some(instance) = snapshot.instances().first() {
            let now = serde_json::to_value(instance.now()).unwrap();
            assert_eq!(now["sections"]["memories"]["total"], 1);
            break instance.now().generated_at().parse::<u128>().unwrap();
        }
    };
    let inventory_url = format!("{origin}/v1/instances/{}/inventory", auth.instance_id());
    let response = reqwest::Client::new()
        .get(&inventory_url)
        .bearer_auth(LOCAL_RAW)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let before: serde_json::Value =
        serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    subscriber.shutdown().await.unwrap();
    // The actual Broker runtime ends its sockets; the TCP listener also goes away.
    runtime.shutdown().await.unwrap();
    server.shutdown().await;
    // A restarted Broker may have lost this registration; recovery must publish it again.
    let deleted =
        sqlx_core::query::query("DELETE FROM public.broker_instances WHERE instance_id=$1")
            .bind(auth.instance_id())
            .execute(&mut connection)
            .await
            .unwrap();
    assert_eq!(deleted.rows_affected(), 1);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let local = reqwest::Client::new()
        .get(format!("http://{source_address}/"))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .unwrap();
    assert!(local.status().is_client_error());
    assert!(
        !running.is_finished(),
        "a network loss must not end local Source"
    );
    // A real local write succeeds while the Broker is absent.
    add_memory(
        &writer.auth,
        &writer_caller,
        &original,
        writer.scope(&writer.version),
        "written-while-offline",
    )
    .await;
    let runtime = RealtimeServerRuntime::new(Duration::from_secs(4), n(8)).unwrap();
    let server = Server::start(
        TcpListener::bind(&broker_address).await.unwrap(),
        local_routes(store.clone(), &runtime, realtime()),
    );
    let mut subscriber = start_now_subscription(
        &transport,
        &lease,
        &original,
        NowSubscriptionConfig {
            project_id: auth.project_id(),
            instance_id: Some(auth.instance_id()),
            version: None,
            command_bytes: n(65536),
            snapshot_bytes: n(65536),
        },
    )
    .await
    .unwrap();
    loop {
        let snapshot = subscriber.next().await.unwrap();
        if let Some(instance) = snapshot.instances().first() {
            let now = serde_json::to_value(instance.now()).unwrap();
            if now["sections"]["memories"]["total"] == 2 {
                assert_eq!(instance.instance_id(), auth.instance_id());
                assert_eq!(instance.version(), "main");
                assert!(instance.now().generated_at().parse::<u128>().unwrap() > generated);
                assert!(
                    now["sections"]["memories"]["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|item| item["title"] == "written-while-offline")
                );
                break;
            }
        }
    }
    let response = reqwest::Client::new()
        .get(&inventory_url)
        .bearer_auth(LOCAL_RAW)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let after: serde_json::Value =
        serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(after["instance_id"], before["instance_id"]);
    assert_eq!(after["projects"], before["projects"]);
    assert!(
        after["updated_at"].as_i64().unwrap() > before["updated_at"].as_i64().unwrap(),
        "lost Broker inventory must be registered from current state"
    );
    // A genuine current authorization denial remains terminal for its publisher.
    acl(
        &mut connection,
        &["presence:read", "memory:read", "knowledge:read"],
        &ids,
    )
    .await;
    while !subscriber.next().await.unwrap().instances().is_empty() {}
    assert!(!running.is_finished());
    assert_local_source_operational(&auth.paths, source_address, &auth.project_paths, &original)
        .await;
    service.cancel();
    tokio::time::timeout(Duration::from_secs(30), &mut running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    drop(subscriber);
    acl(&mut connection, &capabilities, &ids).await;
    let owner = ForegroundSource::start_and_publish(
        &auth.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        &original,
    )
    .await
    .unwrap();
    let service = Cancellation::default();
    let operational = service.clone();
    let running = tokio::spawn(async move { owner.run(&operational).await });
    let mut subscriber = start_now_subscription(
        &transport,
        &lease,
        &original,
        NowSubscriptionConfig {
            project_id: auth.project_id(),
            instance_id: Some(auth.instance_id()),
            version: None,
            command_bytes: n(65536),
            snapshot_bytes: n(65536),
        },
    )
    .await
    .unwrap();
    loop {
        if !subscriber.next().await.unwrap().instances().is_empty() {
            break;
        }
    }
    subscriber.shutdown().await.unwrap();
    runtime.shutdown().await.unwrap();
    server.shutdown().await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        !running.is_finished(),
        "the restarted owner must be waiting for its Broker"
    );
    service.cancel();
    tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!auth.paths.root().join("daemon/mcp.json").exists());
    assert!(!auth.paths.root().join("daemon/mcp.key").exists());
    connection.close().await.unwrap();
    store.shutdown(pg::deadline()).await.unwrap();
    drop(store);
    fixture.shutdown().unwrap();
}

struct RefreshEndpoint {
    origin: String,
    key: Arc<Ed25519KeyPair>,
    calls: std::sync::atomic::AtomicUsize,
    mode: std::sync::atomic::AtomicU8,
    response_at: std::sync::atomic::AtomicU64,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
impl RefreshEndpoint {
    fn signed(&self, claims: serde_json::Value) -> String {
        let message = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(json!({"alg":"EdDSA","kid":"actual","typ":"JWT"}).to_string()),
            URL_SAFE_NO_PAD.encode(claims.to_string())
        );
        format!(
            "{message}.{}",
            URL_SAFE_NO_PAD.encode(self.key.sign(message.as_bytes()).as_ref())
        )
    }
    fn credential(&self, expiry: u64) -> Credential {
        let access = self.signed(json!({"iss":self.origin,"sub":"fixture-sub","aud":["broker-api"],"client_id":"graphit-cli","jti":"refresh-initial","token_use":"access","scope":"presence-sdk","iat":seconds().saturating_sub(1),"exp":expiry}));
        let id = self.signed(json!({"iss":self.origin,"sub":"fixture-sub","aud":"graphit-cli","iat":seconds().saturating_sub(1),"exp":expiry,"preferred_username":"fixture"}));
        Credential::Renewable {
            access,
            refresh: Some("refresh-initial".into()),
            id,
            expires_at: i64::try_from(Duration::from_secs(expiry).as_micros()).unwrap(),
        }
    }
    async fn respond(
        &self,
        body: axum::body::Bytes,
    ) -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
        let body = std::str::from_utf8(&body).unwrap();
        assert!(
            body.split('&')
                .any(|item| item == "grant_type=refresh_token")
        );
        assert!(
            body.split('&')
                .any(|item| item == "refresh_token=refresh-initial")
        );
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.entered.notify_one();
        let mode = self.mode.load(std::sync::atomic::Ordering::SeqCst);
        if mode == 1 {
            self.release.notified().await;
        }
        if mode == 2 {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(json!({"error":"temporarily_unavailable"})),
            );
        }
        let now = seconds();
        self.response_at.store(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                .try_into()
                .unwrap(),
            std::sync::atomic::Ordering::SeqCst,
        );
        let access = self.signed(json!({"iss":self.origin,"sub":"fixture-sub","aud":["broker-api"],"client_id":"graphit-cli","jti":"refresh-next","token_use":"access","scope":"presence-sdk","iat":now,"exp":now+3600}));
        let id = self.signed(json!({"iss":self.origin,"sub":"fixture-sub","aud":"graphit-cli","iat":now,"exp":now+3600,"preferred_username":"fixture"}));
        (
            axum::http::StatusCode::OK,
            axum::Json(
                json!({"access_token":access,"id_token":id,"refresh_token":"refresh-next","token_type":"Bearer","expires_in":3600}),
            ),
        )
    }
    fn count(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

fn foreground_config(paths: &InstallationPaths, disabled: &[Module]) -> ConfigSnapshot {
    let mut values = vec![(
        ConfigKey::new("broker.allow_loopback_http").unwrap(),
        json!(true),
    )];
    values.extend(disabled.iter().map(|module| (module.key(), json!(false))));
    ConfigSnapshot::new(
        paths.brand(),
        [ConfigDocument::new(paths.brand(), ConfigSource::Command, values).unwrap()],
    )
    .unwrap()
}
fn token_revision(auth: &ReleasedInstallation, original: &RequestContext<'_>) -> u64 {
    let id = InstallationIdentityStore::new(&auth.paths)
        .unwrap()
        .load()
        .unwrap()
        .unit_id()
        .unwrap()
        .clone();
    let scope = CapturedPublicationScope::installation(
        &auth.paths,
        id,
        [auth.paths.root().to_path_buf()],
        ScopeLimits::new(16, 4096, 128).unwrap(),
        original.snapshot(),
    )
    .unwrap();
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            scope,
            AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
            original.snapshot(),
        )
        .unwrap(),
        ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 4,
            profiles: 4,
        },
        original,
    )
    .unwrap();
    store
        .load(original)
        .unwrap()
        .document()
        .profile("now")
        .unwrap()
        .token_revision()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real Native rotation/Auth CAS/Core/Broker TCP WS/PostgreSQL TLS NOSUPERUSER"]
async fn native_foreground_refresh_two_projects() {
    let NativeBroker {
        fixture,
        mut connection,
        store,
        key: _,
        origin,
        raw,
        server,
        tokens,
        _runtime,
    } = native_broker(realtime()).await;
    let cancel = Cancellation::default();
    let original = request(&cancel, Duration::from_secs(180));
    let capabilities = [
        "presence:read",
        "presence:publish",
        "task:read",
        "memory:read",
        "knowledge:read",
    ];
    let auth = AuthFixture::new(
        &origin,
        tokens.credential(seconds() + 15),
        None,
        &ordinary(),
        &original,
    );
    let second = AuthFixture::new(&origin, Credential::Local, None, &ordinary(), &original);
    let ids = [auth.project_id(), second.project_id()];
    assert_ne!(ids[0], ids[1]);
    acl(&mut connection, &capabilities, &ids).await;
    let writer = AuthFixture::new(
        &origin,
        Credential::Native(raw.to_string()),
        None,
        &ordinary(),
        &original,
    );
    let auth = auth.release(&original);
    let revision = token_revision(&auth, &original);
    let config = foreground_config(
        &auth.paths,
        &[Module::Ast, Module::Memory, Module::Knowledge],
    );
    let inputs = [
        ProjectBootstrapInput {
            project: &auth.project_paths,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        },
        ProjectBootstrapInput {
            project: &second.project_paths,
            explicit_version: "branch/ação",
            pin: None,
            detached_base: "",
        },
    ];
    let owner = ForegroundSource::start_and_publish(
        &auth.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        &original,
    )
    .await
    .unwrap();
    let service = Cancellation::default();
    let operational = service.clone();
    let running = tokio::spawn(async move { owner.run(&operational).await });
    let caller = writer
        .auth
        .issuer()
        .authenticate_runtime(&writer.runtime_key(&original), &original)
        .unwrap();
    let mut selection = writer
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "now")
        .unwrap();
    let cache = cache();
    let verifier = cache.get(&mut selection).unwrap();
    let lease = selection.native_credential(&verifier).await.unwrap();
    let transport = websocket(&origin);
    let mut subscribers = Vec::new();
    for id in ids {
        subscribers.push(
            start_now_subscription(
                &transport,
                &lease,
                &original,
                NowSubscriptionConfig {
                    project_id: id,
                    instance_id: Some(auth.instance_id()),
                    version: None,
                    command_bytes: n(65536),
                    snapshot_bytes: n(65536),
                },
            )
            .await
            .unwrap(),
        );
    }
    let mut before = Vec::new();
    for subscriber in &mut subscribers {
        let timestamp = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Some(instance) = subscriber.next().await.unwrap().instances().first() {
                    break instance.now().generated_at().parse::<u128>().unwrap();
                }
            }
        })
        .await
        .unwrap();
        before.push(timestamp);
    }
    // This branch must exercise expiration of already-live channels, not bootstrap renewal.
    assert_eq!(tokens.count(), 0);
    assert_eq!(token_revision(&auth, &original), revision);
    tokio::time::timeout(Duration::from_secs(30), tokens.entered.notified())
        .await
        .unwrap();
    for (subscriber, timestamp) in subscribers.iter_mut().zip(before) {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let snapshot = subscriber.next().await.unwrap();
                if snapshot.instances().first().is_some_and(|instance| {
                    let response_at =
                        u128::from(tokens.response_at.load(std::sync::atomic::Ordering::SeqCst));
                    instance.now().generated_at().parse::<u128>().unwrap()
                        > timestamp.max(response_at)
                }) && token_revision(&auth, &original) == revision + 1
                {
                    break;
                }
            }
        })
        .await
        .unwrap();
    }
    assert!(!running.is_finished());
    assert_eq!(tokens.count(), 1);
    assert_eq!(token_revision(&auth, &original), revision + 1);
    service.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(20), running)
            .await
            .unwrap()
            .unwrap()
            .is_ok()
    );
    for subscriber in subscribers {
        subscriber.shutdown().await.unwrap();
    }
    assert!(!auth.paths.root().join("daemon/mcp.json").exists());
    assert!(!auth.paths.root().join("daemon/mcp.key").exists());
    server.shutdown().await;
    connection.close().await.unwrap();
    store.shutdown(pg::deadline()).await.unwrap();
    drop(store);
    fixture.shutdown().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real Native refresh cancellation/error/Auth CAS/Core/Broker TCP/PostgreSQL TLS NOSUPERUSER"]
async fn native_foreground_refresh_cancellation_and_remote_failure_do_not_retry() {
    let NativeBroker {
        fixture,
        mut connection,
        store,
        key: _,
        origin,
        raw: _,
        server,
        tokens,
        _runtime,
    } = native_broker(realtime()).await;
    let signal = Cancellation::default();
    let original = request(&signal, Duration::from_secs(120));
    let capabilities = ["presence:read", "presence:publish"];
    for mode in [1, 2] {
        tokens.mode.store(mode, std::sync::atomic::Ordering::SeqCst);
        let auth = AuthFixture::new(
            &origin,
            tokens.credential(seconds() - 1),
            None,
            &ordinary(),
            &original,
        );
        acl(&mut connection, &capabilities, &[auth.project_id()]).await;
        let auth = auth.release(&original);
        let prior = token_revision(&auth, &original);
        let config = foreground_config(&auth.paths, &[]);
        let inputs = [ProjectBootstrapInput {
            project: &auth.project_paths,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        }];
        let stop = Cancellation::default();
        let operation = request(&stop, Duration::from_secs(30));
        let before = tokens.count();
        let start = ForegroundSource::start_and_publish(
            &auth.paths,
            &inputs,
            0,
            &config,
            NativeBinding::KiroV3,
            Vec::new(),
            &operation,
        );
        tokio::pin!(start);
        let mut failure = if mode == 1 {
            tokio::select! {
                biased;
                () = tokens.entered.notified() => (),
                result = &mut start => panic!("refresh ended before controlled response: {}", result.is_ok()),
            }
            stop.cancel();
            let failure = tokio::time::timeout(Duration::from_secs(10), &mut start)
                .await
                .unwrap()
                .err()
                .expect("bootstrap refresh must fail");
            tokens.release.notify_one();
            failure
        } else {
            tokio::time::timeout(Duration::from_secs(10), &mut start)
                .await
                .unwrap()
                .err()
                .expect("bootstrap refresh must fail")
        };
        if mode == 1 {
            let terminal = failure.service.as_ref().unwrap_or(&failure.error);
            assert_eq!(terminal.kind(), std::io::ErrorKind::Interrupted);
        }
        // Source reports a causal cancellation error before releasing its runtime owner.
        // Consume the returned cleanup owner; this must not perform another token POST.
        if let Some(owner) = failure.owner.take() {
            eprintln!("retained startup cleanup: {:?}", failure.error);
            tokio::time::timeout(Duration::from_secs(10), owner.shutdown())
                .await
                .unwrap()
                .expect("retained startup cleanup must drain");
        }
        assert!(failure.owner.is_none());
        if mode == 2 {
            let terminal = failure.service.as_ref().unwrap_or(&failure.error);
            let error = terminal
                .get_ref()
                .unwrap()
                .downcast_ref::<graphit_auth::profile_manager::BrokerRefreshError>()
                .unwrap();
            assert_eq!(
                error.remote_effect(),
                graphit_auth::broker::oauth::RemoteEffect::MayHaveApplied
            );
        }
        assert_eq!(tokens.count(), before + 1);
        assert_eq!(token_revision(&auth, &original), prior);
        assert!(!auth.paths.root().join("daemon/mcp.json").exists());
        assert!(!auth.paths.root().join("daemon/mcp.key").exists());
    }
    let mut credential = tokens.credential(seconds() - 1);
    if let Credential::Renewable { refresh, .. } = &mut credential {
        *refresh = None;
    }
    let auth =
        AuthFixture::new(&origin, credential, None, &ordinary(), &original).release(&original);
    let prior = token_revision(&auth, &original);
    let before = tokens.count();
    let config = foreground_config(&auth.paths, &[]);
    let inputs = [ProjectBootstrapInput {
        project: &auth.project_paths,
        explicit_version: "main",
        pin: None,
        detached_base: "",
    }];
    let mut failure = ForegroundSource::start_and_publish(
        &auth.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        &original,
    )
    .await
    .err()
    .expect("missing refresh token must require administrative recovery");
    let terminal = failure.service.as_ref().unwrap_or(&failure.error);
    let error = terminal
        .get_ref()
        .unwrap()
        .downcast_ref::<graphit_auth::profile_manager::BrokerRefreshError>()
        .unwrap();
    assert_eq!(
        error.remote_effect(),
        graphit_auth::broker::oauth::RemoteEffect::NotSent
    );
    if let Some(owner) = failure.owner.take() {
        tokio::time::timeout(Duration::from_secs(10), owner.shutdown())
            .await
            .unwrap()
            .expect("missing-credential startup cleanup must drain");
    }
    assert!(failure.owner.is_none());
    assert_eq!(tokens.count(), before);
    assert_eq!(token_revision(&auth, &original), prior);
    assert!(!auth.paths.root().join("daemon/mcp.key").exists());
    server.shutdown().await;
    connection.close().await.unwrap();
    store.shutdown(pg::deadline()).await.unwrap();
    drop(store);
    fixture.shutdown().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real Native expired bootstrap/Auth CAS/Core/Broker TCP/PostgreSQL TLS NOSUPERUSER"]
async fn native_foreground_refresh_expired_bootstrap() {
    let NativeBroker {
        fixture,
        mut connection,
        store,
        key: _,
        origin,
        raw: _,
        server,
        tokens,
        _runtime,
    } = native_broker(realtime()).await;
    let cancellation = Cancellation::default();
    let original = request(&cancellation, Duration::from_secs(180));
    let capabilities = [
        "presence:read",
        "presence:publish",
        "task:read",
        "memory:read",
        "knowledge:read",
    ];
    // A separate installation starts with an actually expired profile: inventory admission is NotSent.
    let expired = AuthFixture::new(
        &origin,
        tokens.credential(seconds() - 1),
        None,
        &ordinary(),
        &original,
    );
    acl(&mut connection, &capabilities, &[expired.project_id()]).await;
    let expired = expired.release(&original);
    let prior = token_revision(&expired, &original);
    let config = foreground_config(&expired.paths, &[]);
    let inputs = [ProjectBootstrapInput {
        project: &expired.project_paths,
        explicit_version: "main",
        pin: None,
        detached_base: "",
    }];
    let owner = ForegroundSource::start_and_publish(
        &expired.paths,
        &inputs,
        0,
        &config,
        NativeBinding::KiroV3,
        Vec::new(),
        &original,
    )
    .await
    .unwrap();
    assert_eq!(token_revision(&expired, &original), prior + 1);
    assert_eq!(tokens.count(), 1);
    let stopped = Cancellation::default();
    stopped.cancel();
    assert!(owner.run(&stopped).await.is_ok());
    assert!(!expired.paths.root().join("daemon/mcp.key").exists());
    server.shutdown().await;
    connection.close().await.unwrap();
    store.shutdown(pg::deadline()).await.unwrap();
    drop(store);
    fixture.shutdown().unwrap();
}
