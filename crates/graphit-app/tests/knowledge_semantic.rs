//! Authorized Source → provider embeddings → Knowledge NN publication and reindex.
use graphit_app::embedding_service::EmbeddingMetadata;
use graphit_app::knowledge_semantic::*;
use graphit_daemon::model_service as model_api;
#[path = "../../graphit-daemon/tests/support/model_fixture.rs"]
pub mod descriptor_fixture;
use graphit_auth::*;
use graphit_auth::{profile::*, store::AuthStore};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CapturedSourceReader, CopyLimits,
        ProjectIgnoreCriteria, ProjectSelectionCriteria, ProjectSelectionLimits,
        ScopeBindingRecord, ScopeLimits, SourceReadLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    native_path::NativePath,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::{Checkout, VersionContext},
};
use graphit_data::knowledge_document_lifecycle::{
    KnowledgeDocumentLifecycleLimits, KnowledgeSourceIdentity, KnowledgeSourceOrigins,
    read_native_knowledge_source_origins, reconcile_native_knowledge_source_file,
    remove_native_knowledge_source_origins,
};
use graphit_data::knowledge_document_update::{
    KnowledgeDocumentAuthoredUpdate, KnowledgeDocumentReferenceUpdate,
    KnowledgeDocumentUpdateResult,
};
use graphit_data::knowledge_embeddings::*;
use graphit_data::{knowledge_document_projection as h, knowledge_index::*};
use graphit_hub::{
    ArtifactType, NativeBinding, ProjectionLimits, SourceOrigin, SourceRecord, SourceRef,
    native::{ContentLimits, Role, RoleCredentialReference},
    policy::RequestContext,
};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, UNIX_EPOCH},
};

// Retain this fixture's real Local SDK and connection fault injection while the
// production Knowledge pipeline consumes provider-independent vector callbacks.
struct LocalInference<'a, 'lease> {
    client: &'a graphit_daemon::model_service::ModelServiceClient,
    lease: &'a LocalServiceLease<'lease>,
    runtime: &'a RuntimeKeyOwner,
    expected: &'a graphit_daemon::model_service::ExpectedModel,
    rpc_timeout: Duration,
}
impl LocalInference<'_, '_> {
    fn pipeline<'a>(&self, metadata: &'a EmbeddingMetadata) -> KnowledgeSemanticModel<'a> {
        let mut batch_limits = self.client.limits();
        if self.expected.embedding_is_scalar() {
            batch_limits.max_texts = 1;
        }
        KnowledgeSemanticModel {
            metadata,
            batch_limits,
            rpc_timeout: self.rpc_timeout,
        }
    }
    async fn infer<S, F>(
        &self,
        operation: graphit_daemon::model_service::EmbedOperation,
        connect: F,
    ) -> Result<Vec<Vec<f32>>, graphit_daemon::model_service::ModelServiceError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
        F: std::future::Future<Output = std::io::Result<S>>,
    {
        self.client
            .embed(self.lease, self.runtime, self.expected, operation, connect)
            .await
            .map(|output| output.vectors)
    }
}

fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 65536,
            max_scalar_bytes: 4096,
            max_collection_entries: 128,
        },
        cancel,
        Instant::now() + Duration::from_secs(900),
    )
    .unwrap()
}
fn issuer_limits() -> IssuerLimits {
    IssuerLimits {
        max_permissions: 64,
        max_scope_bytes: 65536,
        max_parent_bearer_bytes: 1024,
    }
}
fn permissions() -> Vec<Permission> {
    vec![
        Permission::Read(Module::Memory),
        Permission::Read(Module::Task),
        Permission::Read(Module::Knowledge),
        Permission::MutateModule(Module::Knowledge),
        Permission::WriteMemory,
        Permission::WriteTask,
        Permission::ManageAuthProfiles,
        Permission::ManageAccessGrants,
        Permission::Delegate,
    ]
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    runtime: Arc<RuntimeKeyOwner>,
    installation: CapturedPublicationScope,
    installation_record: ScopeBindingRecord,
    project: CapturedPublicationScope,
    project_record: ScopeBindingRecord,
    version: VersionContext,
    global: Arc<graphit_daemon::model_child::GlobalModelConfig>,
}
impl Fixture {
    fn new(ctx: &RequestContext<'_>, model_directory: &std::path::Path) -> Self {
        let dir = tempfile::tempdir_in(model_directory).unwrap();
        let root = dir.path().join("project");
        let home = dir.path().join("home");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&home).unwrap();
        let install = InstallationPaths::resolve(
            Brand::new("knowledge-test").unwrap(),
            &root,
            Some(&home),
            None,
        )
        .unwrap();
        let private = installation_bootstrap::prepare(
            &install,
            BootstrapLimits::new(65536, 256).unwrap(),
            ctx.snapshot(),
        )
        .into_installation()
        .unwrap();
        let runtime = Arc::new(
            RuntimeKeyOwner::start(
                private,
                RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
                ctx.snapshot(),
            )
            .into_owner()
            .ok()
            .unwrap(),
        );
        let paths = ProjectPaths::at(&root, &install).unwrap();
        let limits = ScopeLimits::new(16, 4096, 128).unwrap();
        let installation = CapturedPublicationScope::installation(
            &install,
            InstallationId::new("local-installation").unwrap(),
            [root.clone()],
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let project = CapturedPublicationScope::project(
            &paths,
            "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let version = VersionContext::select(&paths, Checkout::NonGit, "", None, "").unwrap();
        let installation_record = installation.binding().unwrap();
        let project_record = project.binding().unwrap();
        let options = ChildOptions::standard().unwrap();
        let global = Arc::new(
            graphit_daemon::model_child::GlobalModelConfig::new(
                installation.owner_root().to_owned(),
                options.capture,
                options.source,
                options.manifest,
            )
            .unwrap(),
        );
        Self {
            _dir: dir,
            root,
            runtime,
            installation,
            installation_record,
            project,
            project_record,
            version,
            global,
        }
    }
    fn realm(&self) -> ScopeCapture<'_> {
        ScopeCapture {
            live: &self.installation,
            record: &self.installation_record,
        }
    }
    fn scope(&self) -> ScopeInput<'_> {
        ScopeInput {
            installation: self.realm(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.project,
                    record: &self.project_record,
                },
                version: &self.version,
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        }
    }
    fn issuer(&self, ctx: &RequestContext<'_>) -> RoleIssuer {
        self.issuer_with_direct(ctx, None, "fixture-weights-r1")
    }
    fn profile_store(&self, ctx: &RequestContext<'_>) -> Arc<AuthStore> {
        let paths = InstallationPaths::resolve(
            self.installation.brand().clone(),
            &self.root,
            None,
            Some(self.installation.owner_root()),
        )
        .unwrap();
        let scope = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("local-installation").unwrap(),
            [self.installation.owner_root().to_owned()],
            ScopeLimits::new(16, 4096, 128).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let physical = AuthDocumentStore::capture(
            scope,
            AuthDocumentLimits::new(4096, 65536, 256, 65536).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        Arc::new(
            AuthStore::new(
                physical,
                ProfileLimits {
                    bytes: 4096,
                    nodes: 4096,
                    scalar_bytes: 256,
                    collection_entries: 64,
                    providers: 16,
                    profiles: 16,
                },
                ctx,
            )
            .unwrap(),
        )
    }
    fn issuer_with_direct(
        &self,
        ctx: &RequestContext<'_>,
        endpoint: Option<&str>,
        model_revision: &str,
    ) -> RoleIssuer {
        let local = self.profile_store(ctx);
        if local
            .load(ctx)
            .unwrap()
            .document()
            .provider("service")
            .is_none()
        {
            install_service_profile(&local, ctx, endpoint);
        }
        let local_rules = [
            ServiceRuleInput::Local {
                origin: ServiceOrigin::Runtime,
                provider: "service",
                purpose: ServicePurpose::Embedding,
                execution: cpu(),
            },
            ServiceRuleInput::Local {
                origin: ServiceOrigin::Runtime,
                provider: "service",
                purpose: ServicePurpose::Rerank,
                execution: cpu(),
            },
        ];
        let remote_rules = endpoint.map(|endpoint| {
            [ServiceRuleInput::Remote {
                origin: ServiceOrigin::Runtime,
                binding: RemoteServiceBindingInput::Direct {
                    provider: "service",
                    purpose: ServicePurpose::Embedding,
                    protocol: DirectProtocol::OpenAi,
                    endpoint,
                    model: "fixture-embedding",
                    model_revision,
                    recipe: "fixture-query-document-v1",
                    dimensions: 4,
                    request_dimensions: None,
                    batch_endpoint: None,
                },
                credential: CredentialSource::DirectKey,
                incoming: None,
            }]
        });
        let rules = remote_rules
            .as_ref()
            .map_or(local_rules.as_slice(), |rules| rules.as_slice());
        let mut perms = permissions();
        if endpoint.is_some() {
            perms.push(Permission::External(ExternalEffect::RemoteRequest));
        }
        let source = SourceAuthorityPolicy {
            ordinary: &perms,
            administrative: &perms,
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(3600),
                maximum: Duration::from_secs(3600),
            },
        };
        let policy = IssuerAuthorityPolicy::new(
            source,
            source,
            issuer_limits(),
            ServiceAuthorityInput {
                rules,
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 65536,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap();
        RoleIssuer::new(
            self.realm(),
            self.runtime.clone(),
            local,
            policy,
            TtlPolicy {
                ttl: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
                refresh_margin: Duration::from_secs(120),
            },
            ctx,
        )
        .unwrap()
    }
    fn secret(&self) -> String {
        std::fs::read_to_string(self.installation.owner_root().join("daemon/mcp.key")).unwrap()
    }
    fn issue(
        &self,
        issuer: &RoleIssuer,
        parent: &AdminParent<'_>,
        requested: &[Permission],
    ) -> (RoleBearer, RoleCredentialReference) {
        let source = SourceRef::restore(
            SourceRecord {
                remote_file_origin: None,
                archive_sha256: None,
                origin: SourceOrigin::LocalLink,
                producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
                artifact_type: ArtifactType::Agent,
                id: "roles/coordinator-main".into(),
                physical_id: None,
                version: "1.0.0".into(),
                root: NativePath::new(self.root.clone()).unwrap(),
            },
            ProjectionLimits::new(128, 4096, 32, 128, 32).unwrap(),
        )
        .unwrap();
        let perms = permissions();
        let definition = RoleDefinition::new(
            Role::Coordinator,
            &source,
            NativeBinding::KiroV3,
            &perms,
            [1; 32],
            issuer_limits(),
        )
        .unwrap();
        let reference = definition.reference().clone();
        issuer.register_definition(parent, definition).unwrap();
        let run = issuer
            .start_run(parent, &self.scope(), Duration::from_secs(3600))
            .unwrap();
        let bearer = issuer
            .issue(parent, &run, &reference, requested, &self.scope())
            .unwrap();
        (bearer, reference)
    }
}
fn content() -> KnowledgeIndexContentLimits {
    KnowledgeIndexContentLimits {
        max_identifier_bytes: n(256),
    }
}
fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn read_limits() -> KnowledgeIndexReadLimits {
    KnowledgeIndexReadLimits {
        content: content(),
        native: native(),
        batch_rows: NonZeroU32::new(1).unwrap(),
    }
}
fn write_limits() -> KnowledgeIndexWriteLimits {
    let copy = CopyLimits::new(1024 * 1024, 1024, 65536, 16).unwrap();
    KnowledgeIndexWriteLimits {
        content: content(),
        native: native(),
        artifacts: ArtifactLimits::new(65536, 4096, copy).unwrap(),
    }
}
fn hlimits() -> h::KnowledgeDocumentProjectionLimits {
    h::KnowledgeDocumentProjectionLimits {
        max_documents: n(8),
        max_observations: n(8),
        max_rows: n(64),
        max_field_bytes: n(65536),
        max_text_bytes: n(131072),
    }
}
fn page<'a>(slug: &'a str, targets: &'a [&'a str]) -> h::AuthoredPageFields<'a> {
    h::AuthoredPageFields {
        slug: Some(slug),
        title: Some(""),
        body: Some("---\ntitle: body literal\n---\ntext\n"),
        kind: Some("page"),
        summary: Some("text"),
        description: None,
        summary_truncated: false,
        source_path: "wiki/document.md",
        physical_digest: None,
        compilation_digest: h::CompilationDigest32::from_bytes([0xff; 32]),
        recipe: h::RecipeFields {
            id: h::AUTHORED_RECIPE_ID,
            parameters: Some(h::AuthoredParameters {
                fallback_title: "fallback",
                kind_on_absent: "page",
                max_summary_bytes: n(16),
            }),
        },
        updated_at: Some(UNIX_EPOCH - Duration::from_nanos(17)),
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: targets,
    }
}

use graphit_ai::{context::InferenceContext, native_embedding::NativeSdkRoot};
use graphit_daemon::{
    model_child::ChildOptions, model_service::*, model_transport::DisconnectTransport,
};
use secrecy::SecretString;
use std::{
    error::Error,
    io,
    net::SocketAddr,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use zeroize::Zeroizing;
async fn capture(
    ctx: &RequestContext<'_>,
    read: &ProjectReadAdmission<'_>,
) -> KnowledgeEmbeddingCapture {
    let capture = match capture_knowledge_embedding_corpus(ctx.snapshot(), read, read_limits())
        .await
        .unwrap()
    {
        KnowledgeEmbeddingRead::Published(value) => *value,
        KnowledgeEmbeddingRead::NoCorpus => panic!("expected Knowledge corpus"),
    };
    while capture
        .next_documents(ctx.snapshot(), read)
        .await
        .unwrap()
        .is_some()
    {}
    capture
}

async fn publish_pages(
    ctx: &RequestContext<'_>,
    write: &ProjectWriteAdmission<'_>,
    pages: &[h::AuthoredPageFields<'_>],
) {
    let prepared = h::prepare(ctx.snapshot(), pages, &[], hlimits()).unwrap();
    publish_native_authored(ctx.snapshot(), write, &prepared, write_limits())
        .await
        .unwrap();
}

async fn reconcile_pages(
    f: &Fixture,
    ctx: &RequestContext<'_>,
    read: &ProjectReadAdmission<'_>,
    write: &ProjectWriteAdmission<'_>,
    pages: &[h::AuthoredPageFields<'_>],
) -> KnowledgeGeneration {
    let docs = f.project.owner_root().join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    for entry in std::fs::read_dir(&docs).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    for page in pages {
        std::fs::write(
            docs.join(format!("{}.md", page.slug.unwrap())),
            page.body.unwrap(),
        )
        .unwrap();
    }
    let source = CapturedSourceReader::capture_selected_project(
        &f.project,
        ProjectSelectionCriteria {
            subtree: Some(Path::new("docs")),
            extras: &[],
            include_readme: false,
            extensions: &[".md"],
            ignore: ProjectIgnoreCriteria {
                protected_defaults: &[],
                custom_control: Some(".wikiignore"),
                git_control: Some(".gitignore"),
            },
        },
        ProjectSelectionLimits {
            max_depth: 32,
            max_input_items: 128,
            max_input_bytes: 65536,
            max_observed_path_bytes: 4096,
            max_selected_file_bytes: 4096,
            max_control_file_bytes: 4096,
        },
        SourceReadLimits::new(4096, 4096, 65536).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    let mut expected = capture(ctx, read).await.source_generation().clone();
    let previous_version = expected.version;
    let KnowledgeSourceOrigins::Published {
        origins: previous_origins,
        ..
    } = read_native_knowledge_source_origins(ctx.snapshot(), read, read_limits())
        .await
        .unwrap()
    else {
        panic!("fixture has a real existing Source generation");
    };
    let lifecycle = KnowledgeDocumentLifecycleLimits {
        read: read_limits(),
        write: write_limits(),
    };
    for page in pages {
        let input = [*page];
        let prepared = h::prepare(ctx.snapshot(), &input, &[], hlimits()).unwrap();
        let references = [KnowledgeDocumentReferenceUpdate {
            slug: page.slug.unwrap(),
            authored: KnowledgeDocumentAuthoredUpdate::Preserve,
        }];
        let batch =
            graphit_data::knowledge_first_publication::encode_native_knowledge_source_batch(
                ctx.snapshot(),
                write,
                &prepared,
                &references,
                write_limits(),
            )
            .unwrap();
        expected = match reconcile_native_knowledge_source_file(
            ctx.snapshot(),
            read,
            write,
            &expected,
            &source,
            batch,
            lifecycle,
        )
        .await
        .unwrap()
        {
            KnowledgeDocumentUpdateResult::Updated { generation, .. }
            | KnowledgeDocumentUpdateResult::Unchanged { generation } => generation,
        };
    }
    let origins = pages
        .iter()
        .map(|page| page.source_path.to_owned())
        .collect::<std::collections::HashSet<_>>();
    let removed = previous_origins.difference(&origins).cloned().collect();
    expected = match remove_native_knowledge_source_origins(
        ctx.snapshot(),
        read,
        write,
        &expected,
        &source,
        &removed,
        lifecycle,
    )
    .await
    .unwrap()
    {
        KnowledgeDocumentUpdateResult::Updated { generation, .. }
        | KnowledgeDocumentUpdateResult::Unchanged { generation } => generation,
    };
    assert!(
        expected.version > previous_version,
        "fixture must publish the requested source change"
    );
    expected
}

fn cpu() -> LocalExecution {
    LocalExecution {
        device: LocalDevice::Cpu,
        device_id: 0,
    }
}
fn install_service_profile(store: &AuthStore, ctx: &RequestContext<'_>, endpoint: Option<&str>) {
    let before = store.load(ctx).unwrap();
    let result = store
        .mutate(before, ctx, |doc, ctx| {
            doc.add_provider(
                "service".into(),
                ProviderConfig {
                    kind: ProviderKind::Local {
                        allow_aws_credential_chain: false,
                        broker: None,
                    },
                    embedding: endpoint.map_or(AiService::Local(cpu()), |endpoint| {
                        AiService::Direct {
                            protocol: DirectProtocol::OpenAi,
                            endpoint: endpoint.into(),
                            model: "fixture-embedding".into(),
                            model_revision: "fixture-weights-r1".into(),
                            recipe: "fixture-query-document-v1".into(),
                            dimensions: 4,
                            request_dimensions: None,
                            batch_endpoint: None,
                        }
                    }),
                    rerank: AiService::Local(cpu()),
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
            let expectation = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("service").unwrap().revision(),
                profile_revision: None,
            };
            doc.replace_profile_data(
                ProfileData {
                    name: "fixture".into(),
                    provider: "service".into(),
                    username: "fixture".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new(SecretString::from("fixture-key-only"))),
                        broker_key: None,
                        embedding_api_key: endpoint.map(|_| {
                            Secret::new(SecretString::from("fixture-remote-embedding-key"))
                        }),
                        rerank_api_key: None,
                        s3: None,
                    }),
                },
                expectation,
                1,
                ctx,
            )?;
            doc.activate("fixture", doc.revision(), ctx)?;
            Ok(())
        })
        .unwrap();
    assert!(result.failure().is_none());
    assert!(result.cleanup_failure().is_none());
}

fn model_config(
    bundle: &Path,
    dimensions: u64,
    max_tokens: u64,
    ctx: &RequestContext<'_>,
) -> ResolvedConfig {
    let manifest_relative = format!("{dimensions}-{max_tokens}.json");
    let digest = [
        0x68, 0xba, 0xe2, 0x9d, 0x62, 0xfb, 0x8c, 0x7d, 0x23, 0xe9, 0x8d, 0x21, 0xfd, 0x46, 0x62,
        0x75, 0x3d, 0xdd, 0x63, 0x6e, 0x6b, 0x62, 0xb8, 0xa7, 0x0d, 0xfe, 0x05, 0x9a, 0x98, 0x44,
        0xf2, 0x16,
    ];
    let manifest = serde_json::json!({"schema":"graphit-model/v1","id":"embeddinggemma2","revision":"bfcd298762cc34d0357ece5ebdd31791a3a374d8",
        "task":"embedding","runtime":"gguf-embeddinggemma2","model_role":"model",
        "artifacts":[{"role":"model","path":"model.gguf","digest":digest,"minimum_bytes":557950176u64,"required":true,"format":"gguf"}],
        "recipe":{"architecture":"gemma-embedding2","tokenizer":"embedded","dimensions":dimensions,"max_tokens":max_tokens,"precision":"bf16",
            "attention":"noncausal","pooling":"mean","normalization":"f32_l2","mrl":"truncate_then_f32_l2",
            "query_prefix":"task: search result | query: ","document_prefix":"title: none | text: ",
            "code_query_prefix":"task: code retrieval | query: ","code_document_prefix":"title: none | text: "}});
    std::fs::write(
        bundle.join(&manifest_relative),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let options = ChildOptions::standard().unwrap();
    let settings = ModelSettings {
        slots: vec![SlotSettings {
            name: "embedding".into(),
            selection: SelectionSettings {
                id: "embeddinggemma2".into(),
                revision: "bfcd298762cc34d0357ece5ebdd31791a3a374d8".into(),
                task: TaskKind::Embedding,
            },
            bundle: bundle.to_owned(),
            manifest_relative: manifest_relative.into(),
            runtime: RuntimeSettings::EmbeddingGemma2 {
                threads: NonZeroU32::new(2).unwrap(),
                queue_capacity: n(8),
            },
        }],
    };
    let input = CapturedModelInput::capture(
        "embedding",
        bundle,
        &options.capture,
        options.source,
        ctx.snapshot().clone(),
    )
    .unwrap();
    ResolvedConfig::resolve(
        settings,
        LocalExecutionSelection {
            embedding: Some(cpu()),
            rerank: None,
        },
        [input],
        ctx.snapshot(),
        &options.manifest,
    )
    .unwrap()
}

type RpcTask = JoinHandle<ModelResult<()>>;
fn rpc_connect(
    service: &ModelService,
    f: &Fixture,
    ctx: &RequestContext<'_>,
    limits: RpcLimits,
    tasks: &mut Vec<RpcTask>,
) -> impl std::future::Future<Output = io::Result<TcpStream>> + use<> {
    let bound = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address: SocketAddr = bound.local_addr().unwrap();
    bound.set_nonblocking(true).unwrap();
    let listener = TcpListener::from_std(bound).unwrap();
    let runtime = f.runtime.check(ctx.snapshot()).unwrap();
    let mut secret = Zeroizing::new([0; 64]);
    secret.copy_from_slice(runtime.secret(ctx.snapshot()).unwrap());
    let secret = Arc::new(RuntimeSecret::new(secret).unwrap());
    let service = service.clone();
    let global = Arc::clone(&f.global);
    let cancellation = Cancellation::default();
    tasks.push(tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let transport =
            DisconnectTransport::new(server_transport(stream, limits)?, cancellation.clone());
        serve_connection(transport, service, secret, limits, global, cancellation).await
    }));
    async move { TcpStream::connect(address).await }
}
async fn join_rpc(tasks: &mut Vec<RpcTask>) {
    for task in tasks.drain(..) {
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
fn index_limits() -> KnowledgeSemanticIndexLimits {
    KnowledgeSemanticIndexLimits {
        read: read_limits(),
        write: write_limits(),
    }
}
fn query_options() -> KnowledgeSemanticQueryOptions {
    KnowledgeSemanticQueryOptions {
        top_k: n(2),
        metric: KnowledgeEmbeddingMetric::Cosine,
    }
}
fn assert_hits(result: KnowledgeSemanticQueryResult, corpus: &KnowledgeEmbeddingCapture) {
    let KnowledgeSemanticQueryResult::Published(KnowledgeSemanticQuery::Hits {
        source_generation,
        semantic_generation,
        hits,
    }) = result
    else {
        panic!("expected healthy NN result: {result:?}")
    };
    assert_eq!(&source_generation, corpus.source_generation());
    assert_eq!(Some(semantic_generation), corpus.semantic_generation());
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| h.distance.is_finite()));
    let mut slugs: Vec<_> = hits.iter().map(|h| h.slug.as_str()).collect();
    slugs.sort_unstable();
    assert_eq!(slugs, ["database", "paris"]);
}

#[test]
#[ignore = "requires pinned BF16 GGUF in GRAPHIT_EMBEDDINGGEMMA2_MODEL"]
fn authorized_gguf_corpus_index_query_reindex_errors_and_shutdown() {
    // Unoptimized Arrow/Lance encoding exceeds libtest's default thread stack.
    std::thread::Builder::new()
        .name("knowledge-native".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(authorized_gguf_scenario());
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn authorized_gguf_scenario() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let pinned = PathBuf::from(
        std::env::var_os("GRAPHIT_EMBEDDINGGEMMA2_MODEL").expect("pinned BF16 fixture"),
    );
    let f = Fixture::new(&ctx, pinned.parent().expect("pinned model directory"));
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue(&issuer, &parent, &permissions());
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let mut rerank = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let rerank_lease = rerank.local().unwrap();
    let bundle = f.installation.owner_root().join("models/gemma α");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::hard_link(pinned, bundle.join("model.gguf")).unwrap();
    let config = model_config(&bundle, 768, 8192, &ctx);
    let expected = ExpectedModel::new(&config, "embedding", "service").unwrap();
    assert_eq!(expected.dimensions(), Some(768));
    assert!(expected.embedding_is_scalar());
    let options = ChildOptions::standard().unwrap();
    let limits = options.rpc;
    let client = ModelServiceClient::new(limits).unwrap();
    let model = LocalInference {
        client: &client,
        lease: &lease,
        runtime: &f.runtime,
        expected: &expected,
        rpc_timeout: ctx.remaining().unwrap().unwrap(),
    };
    let sdk = NativeSdkRoot::new().unwrap();
    let snapshot_parent = Arc::new(
        graphit_core::native_document_file::CapturedDocumentDirectory::capture(
            f.installation.owner_root(),
            options.bootstrap.native_limits(),
            ctx.snapshot(),
        )
        .unwrap(),
    );
    let service = ModelService::new(snapshot_parent, options.service).unwrap();
    service
        .install(
            config,
            InferenceContext::new(&cancel, ctx.snapshot().required_deadline().unwrap()),
        )
        .unwrap();
    let mut tasks = Vec::new();
    let connected = AtomicUsize::new(0);
    let polled = AtomicUsize::new(0);
    let failed_connect = || {
        connected.fetch_add(1, Ordering::SeqCst);
        async {
            polled.fetch_add(1, Ordering::SeqCst);
            Err::<TcpStream, _>(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                "fixture unavailable transport",
            ))
        }
    };
    assert!(matches!(
        index_knowledge_semantics(
            ctx.snapshot(),
            &read,
            &write,
            &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
            KnowledgeEmbeddingWriteMode::Attach,
            index_limits(),
            |operation| model.infer(operation, (failed_connect)())
        )
        .await
        .unwrap(),
        KnowledgeSemanticIndexResult::NoCorpus
    ));
    assert!(matches!(
        query_knowledge_semantics(
            ctx.snapshot(),
            &read,
            &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
            "  Paris?\r\n",
            query_options(),
            read_limits(),
            |operation| model.infer(operation, (failed_connect)())
        )
        .await
        .unwrap(),
        KnowledgeSemanticQueryResult::NoCorpus
    ));
    assert_eq!(connected.load(Ordering::SeqCst), 0);
    assert!(
        !f.project
            .runtime_root()
            .join("knowledge/index.lance")
            .exists()
    );
    let mut paris = page("paris", &[]);
    let paris_source =
        KnowledgeSourceIdentity::from_relative_path(Path::new("docs/paris.md"), None, n(256))
            .unwrap();
    paris.source_path = paris_source.source_path();
    paris.title = Some("Paris");
    paris.summary = Some("France");
    paris.body = Some("Paris is the capital of France.\r\n");
    let mut database = page("database", &[]);
    let database_source =
        KnowledgeSourceIdentity::from_relative_path(Path::new("docs/database.md"), None, n(256))
            .unwrap();
    database.source_path = database_source.source_path();
    database.title = Some("Database");
    database.summary = Some("Indexes");
    database.body = Some("Database indexes make queries efficient.\n");
    publish_pages(&ctx, &write, &[paris, database]).await;

    let foreign = Arc::new(graphit_core::file_io::SnapshotContext::new(
        ctx.snapshot().required_deadline().unwrap(),
        cancel.clone(),
    ));
    assert!(matches!(
        index_knowledge_semantics(
            &foreign,
            &read,
            &write,
            &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
            KnowledgeEmbeddingWriteMode::Attach,
            index_limits(),
            |operation| model.infer(operation, (failed_connect)())
        )
        .await
        .unwrap_err(),
        KnowledgeSemanticError::ForeignRequest
    ));
    let expired = Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    ));
    assert!(matches!(query_knowledge_semantics(&expired,
&read,
&model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
"Paris",
query_options(),
read_limits(),
|operation| model.infer(operation, (failed_connect)())).await.unwrap_err(), KnowledgeSemanticError::Request(ref e) if e.kind() == io::ErrorKind::TimedOut));
    let task_read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let task_write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    assert!(matches!(
        query_knowledge_semantics(
            ctx.snapshot(),
            &task_read,
            &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
            "Paris",
            query_options(),
            read_limits(),
            |operation| model.infer(operation, (failed_connect)())
        )
        .await
        .unwrap_err(),
        KnowledgeSemanticError::InvalidAdmission
    ));
    assert!(matches!(
        index_knowledge_semantics(
            ctx.snapshot(),
            &read,
            &task_write,
            &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
            KnowledgeEmbeddingWriteMode::Attach,
            index_limits(),
            |operation| model.infer(operation, (failed_connect)())
        )
        .await
        .unwrap_err(),
        KnowledgeSemanticError::InvalidAdmission
    ));
    let readonly_perms = [
        Permission::Read(Module::Memory),
        Permission::Read(Module::Task),
        Permission::Read(Module::Knowledge),
    ];
    let readonly_run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let readonly = issuer
        .issue(
            &parent,
            &readonly_run,
            &reference,
            &readonly_perms,
            &f.scope(),
        )
        .unwrap();
    let readonly_grant = issuer
        .verify(readonly.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    assert!(
        authority
            .admit_read(&readonly_grant, &ctx, ProjectReadModule::Knowledge)
            .is_ok()
    );
    assert!(matches!(
        authority.admit_write(&readonly_grant, &ctx, ProjectWriteOperation::Knowledge),
        Err(AuthError::Denied)
    ));
    let error = client
        .embed(
            &rerank_lease,
            &f.runtime,
            &expected,
            EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["Paris".into()],
                deadline: ctx.snapshot().required_deadline().unwrap(),
            },
            std::future::ready(Err::<TcpStream, _>(io::Error::from(
                io::ErrorKind::ConnectionRefused,
            ))),
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ModelErrorKind::Authentication);
    assert!(matches!(
        error.source().unwrap().downcast_ref::<ServiceUseError>(),
        Some(ServiceUseError::WrongMode)
    ));
    assert_eq!(connected.load(Ordering::SeqCst), 0);
    let mut bounded = read_limits();
    bounded.content.max_identifier_bytes = n(1);
    let error = query_knowledge_semantics(
        ctx.snapshot(),
        &read,
        &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
        "Paris",
        query_options(),
        bounded,
        |operation| model.infer(operation, (failed_connect)()),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, KnowledgeSemanticError::Data(_)));
    assert!(error.source().is_some());
    assert_eq!(connected.load(Ordering::SeqCst), 0);
    let error = index_knowledge_semantics(
        ctx.snapshot(),
        &read,
        &write,
        &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
        KnowledgeEmbeddingWriteMode::Attach,
        index_limits(),
        |operation| model.infer(operation, (failed_connect)()),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, KnowledgeSemanticError::Models(ref e) if e.kind() == ModelErrorKind::Transport)
    );
    assert!(error.source().unwrap().source().is_some());
    assert!(capture(&ctx, &read).await.semantic_generation().is_none());
    let tiny_client =
        ModelServiceClient::new(RpcLimits::new(131072, 32, 1, 24576).unwrap()).unwrap();
    let tiny = LocalInference {
        client: &tiny_client,
        ..model
    };
    let polls_before = polled.load(Ordering::SeqCst);
    assert!(matches!(query_knowledge_semantics(ctx.snapshot(),
&read,
&tiny.pipeline(&EmbeddingMetadata::local(tiny.expected).unwrap()),
"oversized",
query_options(),
read_limits(),
|operation| tiny.infer(operation, (failed_connect)()))
        .await.unwrap_err(), KnowledgeSemanticError::Models(ref e) if e.kind() == ModelErrorKind::Limit));
    assert_eq!(polled.load(Ordering::SeqCst), polls_before);

    // A failed second RPC must retain Source and report the schema publication
    // already needed for the first vector, without committing partial vectors.
    let attempts = AtomicUsize::new(0);
    let failed_second = index_knowledge_semantics(
        ctx.snapshot(),
        &read,
        &write,
        &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
        KnowledgeEmbeddingWriteMode::Attach,
        index_limits(),
        |operation| {
            model.infer(operation, {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                let transport =
                    (attempt == 1).then(|| rpc_connect(&service, &f, &ctx, limits, &mut tasks));
                async move {
                    match transport {
                        Some(connect) => connect.await,
                        None => Err(io::Error::new(
                            io::ErrorKind::ConnectionRefused,
                            "second batch fixture",
                        )),
                    }
                }
            })
        },
    )
    .await
    .unwrap_err();
    join_rpc(&mut tasks).await;
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    let KnowledgeSemanticError::Publication { outcome, cause } = &failed_second else {
        panic!("schema effect must remain observable: {failed_second:?}")
    };
    assert!(
        matches!(cause.as_ref(), KnowledgeSemanticError::Models(cause)
        if cause.kind() == ModelErrorKind::Transport)
    );
    let Err(KnowledgeEmbeddingError::CorpusChanged { generation, .. }) = outcome.as_ref() else {
        panic!("failed stream must report its actual schema generation")
    };
    let after_failed_second = capture(&ctx, &read).await;
    assert_eq!(after_failed_second.source_generation(), generation);
    assert_eq!(after_failed_second.stored_inputs().count(), 0);
    assert!(
        f.project
            .runtime_root()
            .join("knowledge/index.lance")
            .is_dir()
    );

    let result = query_knowledge_semantics(
        ctx.snapshot(),
        &read,
        &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
        "  Paris?\r\n",
        query_options(),
        read_limits(),
        |operation| {
            model.infer(
                operation,
                rpc_connect(&service, &f, &ctx, limits, &mut tasks),
            )
        },
    )
    .await
    .unwrap();
    join_rpc(&mut tasks).await;
    assert!(matches!(
        result,
        KnowledgeSemanticQueryResult::Published(KnowledgeSemanticQuery::Unavailable(
            KnowledgeEmbeddingUnavailable::Incomplete
        ))
    ));
    // The scalar backend receives separate RPCs even when the source and client
    // would permit both documents in one batch.
    assert!(limits.max_texts > 1);
    let mut two_documents = index_limits();
    two_documents.read.batch_rows = NonZeroU32::new(2).unwrap();
    let count_connections = AtomicUsize::new(0);
    let result = index_knowledge_semantics(
        ctx.snapshot(),
        &read,
        &write,
        &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
        KnowledgeEmbeddingWriteMode::Attach,
        two_documents,
        |operation| {
            model.infer(operation, {
                count_connections.fetch_add(1, Ordering::SeqCst);
                rpc_connect(&service, &f, &ctx, limits, &mut tasks)
            })
        },
    )
    .await
    .unwrap();
    assert_eq!(count_connections.load(Ordering::SeqCst), 2);
    join_rpc(&mut tasks).await;
    assert!(matches!(
        result,
        KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Committed { .. })
    ));
    let corpus = capture(&ctx, &read).await;
    assert_eq!(corpus.stored_inputs().count(), 2);
    assert!(
        corpus
            .stored_inputs()
            .all(|(_, identity)| identity.vector_id() == expected.vector_id()
                && identity.dimensions().get() == 768)
    );
    let result = query_knowledge_semantics(
        ctx.snapshot(),
        &read,
        &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
        "  Paris?\r\n",
        query_options(),
        read_limits(),
        |operation| {
            model.infer(
                operation,
                rpc_connect(&service, &f, &ctx, limits, &mut tasks),
            )
        },
    )
    .await
    .unwrap();
    join_rpc(&mut tasks).await;
    assert_hits(result, &corpus);
    let connects_before = connected.load(Ordering::SeqCst);
    for mode in [
        KnowledgeEmbeddingWriteMode::Attach,
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
    ] {
        let mut two_documents = index_limits();
        two_documents.read.batch_rows = NonZeroU32::new(2).unwrap();
        assert!(matches!(
            index_knowledge_semantics(
                ctx.snapshot(),
                &read,
                &write,
                &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
                mode,
                two_documents,
                |operation| model.infer(operation, (failed_connect)())
            )
            .await
            .unwrap(),
            KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Unchanged)
        ));
    }
    assert_eq!(connected.load(Ordering::SeqCst), connects_before);
    assert_eq!(
        capture(&ctx, &read).await.semantic_generation(),
        corpus.semantic_generation()
    );

    // One changed input reuses the other vector, then deletion-only prunes its
    // old row without reinferring the surviving input.
    paris.body = Some("Paris remains the capital of France after this edit.\n");
    let mixed_generation = reconcile_pages(&f, &ctx, &read, &write, &[paris, database]).await;
    count_connections.store(0, Ordering::SeqCst);
    let mixed = index_knowledge_semantics(
        ctx.snapshot(),
        &read,
        &write,
        &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        index_limits(),
        |operation| {
            model.infer(operation, {
                count_connections.fetch_add(1, Ordering::SeqCst);
                rpc_connect(&service, &f, &ctx, limits, &mut tasks)
            })
        },
    )
    .await
    .unwrap();
    join_rpc(&mut tasks).await;
    assert_eq!(count_connections.load(Ordering::SeqCst), 1);
    assert!(matches!(
        mixed,
        KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Committed { .. })
    ));
    let mixed_capture = capture(&ctx, &read).await;
    assert_eq!(
        mixed_capture.source_generation().corpus_uri,
        mixed_generation.corpus_uri
    );
    assert!(mixed_capture.source_generation().version > mixed_generation.version);
    assert_eq!(
        mixed_capture.source_generation().version,
        mixed_capture.semantic_generation().unwrap().version
    );
    assert_eq!(mixed_capture.stored_inputs().count(), 2);
    let old_database = corpus
        .stored_inputs()
        .find(|(i, _)| i.slug == "database")
        .unwrap();
    let new_database = mixed_capture
        .stored_inputs()
        .find(|(i, _)| i.slug == "database")
        .unwrap();
    assert_eq!(
        old_database.0.input_fingerprint,
        new_database.0.input_fingerprint
    );
    assert_eq!(old_database.1, new_database.1);
    assert_ne!(
        mixed_capture.semantic_generation(),
        corpus.semantic_generation()
    );
    let pruned_generation = reconcile_pages(&f, &ctx, &read, &write, &[paris]).await;
    let pruned = index_knowledge_semantics(
        ctx.snapshot(),
        &read,
        &write,
        &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        index_limits(),
        |operation| model.infer(operation, (failed_connect)()),
    )
    .await
    .unwrap();
    assert!(matches!(
        pruned,
        KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Unchanged)
    ));
    assert_eq!(connected.load(Ordering::SeqCst), connects_before);
    let surviving = capture(&ctx, &read).await;
    assert_eq!(surviving.source_generation(), &pruned_generation);
    assert_eq!(
        surviving
            .stored_inputs()
            .map(|(i, _)| i.slug)
            .collect::<Vec<_>>(),
        ["paris"]
    );
    assert_ne!(
        surviving.semantic_generation(),
        mixed_capture.semantic_generation()
    );

    for bound in ["input bytes", "output values"] {
        // Bounds apply to fresh inference; each iteration changes both inputs.
        paris.body = Some(bound);
        database.body = Some(bound);
        reconcile_pages(&f, &ctx, &read, &write, &[paris, database]).await;
        let mut largest = 0;
        let KnowledgeEmbeddingRead::Published(source) =
            capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
                .await
                .unwrap()
        else {
            panic!("actual input bounds source")
        };
        while let Some(documents) = source.next_documents(ctx.snapshot(), &read).await.unwrap() {
            let projection =
                graphit_knowledge::embedding_projection::prepare_knowledge_embedding_projection(
                    ctx.snapshot(),
                    &source,
                    &documents,
                    expected.vector_id(),
                    768,
                )
                .unwrap();
            assert!(projection.documents().iter().all(|doc| doc.is_pending()));
            largest = largest.max(
                projection
                    .documents()
                    .iter()
                    .map(|doc| doc.text().len())
                    .max()
                    .unwrap_or(0),
            );
        }
        let bounded_rpc = RpcLimits::new(
            limits.max_frame_bytes,
            limits.max_texts,
            if bound == "input bytes" {
                largest
            } else {
                limits.max_input_bytes
            },
            if bound == "output values" {
                768
            } else {
                limits.max_output_values
            },
        )
        .unwrap();
        let bounded_client = ModelServiceClient::new(bounded_rpc).unwrap();
        let bounded_model = LocalInference {
            client: &bounded_client,
            ..model
        };
        let mut two_documents = index_limits();
        two_documents.read.batch_rows = NonZeroU32::new(2).unwrap();
        let count = AtomicUsize::new(0);
        let result = index_knowledge_semantics(
            ctx.snapshot(),
            &read,
            &write,
            &bounded_model.pipeline(&EmbeddingMetadata::local(bounded_model.expected).unwrap()),
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            two_documents,
            |operation| {
                bounded_model.infer(operation, {
                    count.fetch_add(1, Ordering::SeqCst);
                    rpc_connect(&service, &f, &ctx, bounded_rpc, &mut tasks)
                })
            },
        )
        .await
        .unwrap();
        join_rpc(&mut tasks).await;
        assert_eq!(count.load(Ordering::SeqCst), 2, "{bound}");
        assert!(matches!(result, KnowledgeSemanticIndexResult::Published(_)));
        let after = capture(&ctx, &read).await;
        assert_eq!(after.stored_inputs().count(), 2);
        assert_eq!(
            after.source_generation().corpus_uri,
            source.source_generation().corpus_uri
        );
        assert!(after.source_generation().version > source.source_generation().version);
        assert_eq!(
            after.source_generation().version,
            after.semantic_generation().unwrap().version
        );
    }

    let mut latest_expected = None;
    for (dimensions, tokens, unavailable) in [
        (
            768,
            4096,
            KnowledgeEmbeddingUnavailable::IncompatibleIdentity,
        ),
        (
            128,
            8192,
            KnowledgeEmbeddingUnavailable::IncompatibleDimensions,
        ),
    ] {
        let config = model_config(&bundle, dimensions, tokens, &ctx);
        let changed = ExpectedModel::new(&config, "embedding", "service").unwrap();
        assert_eq!(changed.dimensions(), Some(dimensions));
        assert_ne!(changed.vector_id(), expected.vector_id());
        service
            .install(
                config,
                InferenceContext::new(&cancel, ctx.snapshot().required_deadline().unwrap()),
            )
            .unwrap();
        let before = capture(&ctx, &read).await;
        let error = query_knowledge_semantics(
            ctx.snapshot(),
            &read,
            &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
            "old config",
            query_options(),
            read_limits(),
            |operation| {
                model.infer(
                    operation,
                    rpc_connect(&service, &f, &ctx, limits, &mut tasks),
                )
            },
        )
        .await
        .unwrap_err();
        join_rpc(&mut tasks).await;
        assert!(
            matches!(error, KnowledgeSemanticError::Models(ref e) if matches!(e.kind(), ModelErrorKind::ConfigChanged | ModelErrorKind::Selection))
        );
        assert_eq!(
            capture(&ctx, &read).await.semantic_generation(),
            before.semantic_generation()
        );
        let changed_model = LocalInference {
            expected: &changed,
            ..model
        };
        let result = query_knowledge_semantics(
            ctx.snapshot(),
            &read,
            &changed_model.pipeline(&EmbeddingMetadata::local(changed_model.expected).unwrap()),
            "Paris",
            query_options(),
            read_limits(),
            |operation| {
                changed_model.infer(
                    operation,
                    rpc_connect(&service, &f, &ctx, limits, &mut tasks),
                )
            },
        )
        .await
        .unwrap();
        join_rpc(&mut tasks).await;
        assert!(
            matches!(result, KnowledgeSemanticQueryResult::Published(KnowledgeSemanticQuery::Unavailable(reason)) if reason == unavailable)
        );
        let result = index_knowledge_semantics(
            ctx.snapshot(),
            &read,
            &write,
            &changed_model.pipeline(&EmbeddingMetadata::local(changed_model.expected).unwrap()),
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            index_limits(),
            |operation| {
                changed_model.infer(
                    operation,
                    rpc_connect(&service, &f, &ctx, limits, &mut tasks),
                )
            },
        )
        .await
        .unwrap();
        join_rpc(&mut tasks).await;
        assert!(matches!(
            result,
            KnowledgeSemanticIndexResult::Published(
                KnowledgeEmbeddingPublication::Committed { .. }
            )
        ));
        let after = capture(&ctx, &read).await;
        assert!(
            after
                .stored_inputs()
                .all(|(_, identity)| identity.vector_id() == changed.vector_id()
                    && u64::from(identity.dimensions().get()) == dimensions)
        );
        let result = query_knowledge_semantics(
            ctx.snapshot(),
            &read,
            &changed_model.pipeline(&EmbeddingMetadata::local(changed_model.expected).unwrap()),
            "Paris",
            query_options(),
            read_limits(),
            |operation| {
                changed_model.infer(
                    operation,
                    rpc_connect(&service, &f, &ctx, limits, &mut tasks),
                )
            },
        )
        .await
        .unwrap();
        join_rpc(&mut tasks).await;
        assert_hits(result, &after);
        latest_expected = Some(changed);
    }

    // Real store failure without SDK-internal mutation or a synthetic capture.
    let index = f.project.runtime_root().join("knowledge/index.lance");
    let backup = f.project.runtime_root().join("knowledge/index.backup");
    std::fs::rename(&index, &backup).unwrap();
    std::fs::write(&index, "directory replaced by a file").unwrap();
    let before = connected.load(Ordering::SeqCst);
    let failure = query_knowledge_semantics(
        ctx.snapshot(),
        &read,
        &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
        "Paris",
        query_options(),
        read_limits(),
        |operation| model.infer(operation, (failed_connect)()),
    )
    .await;
    std::fs::remove_file(&index).unwrap();
    std::fs::rename(&backup, &index).unwrap();
    let error = failure.unwrap_err();
    assert!(matches!(error, KnowledgeSemanticError::Data(_)));
    assert!(error.source().is_some());
    assert_eq!(connected.load(Ordering::SeqCst), before);
    // Cancellation after one completed RPC must preserve the documentary
    // generation and its pending vectors, without committing the partial batch.
    paris.body = Some("fresh Paris before second RPC cancellation");
    database.body = Some("fresh database before second RPC cancellation");
    let before_cancel = reconcile_pages(&f, &ctx, &read, &write, &[paris, database]).await;
    let current_model = LocalInference {
        expected: latest_expected.as_ref().unwrap(),
        ..model
    };
    let cancel_attempts = AtomicUsize::new(0);
    let cancelled_batch = index_knowledge_semantics(
        ctx.snapshot(),
        &read,
        &write,
        &current_model.pipeline(&EmbeddingMetadata::local(current_model.expected).unwrap()),
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        index_limits(),
        |operation| {
            current_model.infer(operation, {
                let attempt = cancel_attempts.fetch_add(1, Ordering::SeqCst) + 1;
                if attempt == 2 {
                    cancel.cancel();
                }
                let transport =
                    (attempt == 1).then(|| rpc_connect(&service, &f, &ctx, limits, &mut tasks));
                async move {
                    match transport {
                        Some(connect) => connect.await,
                        None => Err(io::Error::new(
                            io::ErrorKind::Interrupted,
                            "cancelled batch future",
                        )),
                    }
                }
            })
        },
    )
    .await
    .unwrap_err();
    join_rpc(&mut tasks).await;
    assert_eq!(cancel_attempts.load(Ordering::SeqCst), 2);
    assert!(
        matches!(cancelled_batch, KnowledgeSemanticError::Models(ref cause)
        if cause.kind() == ModelErrorKind::Cancelled)
    );
    let uri = url::Url::from_file_path(f.project.runtime_root().join("knowledge/index.lance"))
        .unwrap()
        .to_string();
    let actual =
        graphit_data::lance_branch_store::BranchSnapshot::capture_observed(&uri, None, None)
            .await
            .unwrap();
    assert_eq!(actual.version(), before_cancel.version);
    let before = connected.load(Ordering::SeqCst);
    cancel.cancel();
    assert!(matches!(index_knowledge_semantics(ctx.snapshot(),
&read,
&write,
&model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
KnowledgeEmbeddingWriteMode::Attach,
index_limits(),
|operation| model.infer(operation, (failed_connect)())).await.unwrap_err(), KnowledgeSemanticError::Request(ref e) if e.kind() == io::ErrorKind::Interrupted));
    assert_eq!(connected.load(Ordering::SeqCst), before);
    assert!(tasks.is_empty());
    let cleanup = Cancellation::default();
    service
        .shutdown(InferenceContext::new(
            &cleanup,
            Instant::now() + Duration::from_secs(30),
        ))
        .await
        .unwrap();
    drop(service);
    drop(sdk);
}

#[test]
fn semantic_identifier_limit_preserves_data_cause_before_transport() {
    std::thread::Builder::new()
        .name("knowledge-semantic-preflight".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let cancel = Cancellation::default();
                    let ctx = request(&cancel);
                    // Only metadata/integrity reaches this scenario: the real
                    // client fails to connect before any inference is possible.
                    let descriptor = descriptor_fixture::Fixture::metadata(TaskKind::Embedding);
                    let workspace = tempfile::tempdir().unwrap();
                    let f = Fixture::new(&ctx, workspace.path());
                    let issuer = f.issuer(&ctx);
                    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
                    let admin = issuer.bootstrap_admin(&caller).unwrap();
                    let parent = issuer.admin_parent(&caller, &admin).unwrap();
                    let (bearer, reference) = f.issue(&issuer, &parent, &permissions());
                    let grant = issuer
                        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
                        .unwrap();
                    let authority = ProjectRecordAuthority::new(&issuer);
                    let read = authority
                        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
                        .unwrap();
                    let write = authority
                        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
                        .unwrap();
                    let mut selection = issuer
                        .capture_service(&caller, ServicePurpose::Embedding)
                        .unwrap();
                    let lease = selection.local().unwrap();
                    let config = descriptor.config_with_original(&["embedding"], ctx.snapshot());
                    let expected = ExpectedModel::new(&config, "embedding", "service").unwrap();
                    let client =
                        ModelServiceClient::new(ChildOptions::standard().unwrap().rpc).unwrap();
                    let model = LocalInference {
                        client: &client,
                        lease: &lease,
                        runtime: &f.runtime,
                        expected: &expected,
                        rpc_timeout: ctx.remaining().unwrap().unwrap(),
                    };
                    let slug = "é".repeat(17);
                    publish_pages(&ctx, &write, &[page(&slug, &[])]).await;
                    let connected = AtomicUsize::new(0);
                    let polled = AtomicUsize::new(0);
                    let failed_connect = || {
                        connected.fetch_add(1, Ordering::SeqCst);
                        async {
                            polled.fetch_add(1, Ordering::SeqCst);
                            Err::<TcpStream, _>(io::Error::new(
                                io::ErrorKind::ConnectionRefused,
                                "fixture unavailable transport",
                            ))
                        }
                    };
                    let mut limits = read_limits();
                    limits.content.max_identifier_bytes = n(slug.len() - 1);
                    let error = query_knowledge_semantics(
                        ctx.snapshot(),
                        &read,
                        &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
                        "Paris",
                        query_options(),
                        limits,
                        |operation| model.infer(operation, (failed_connect)()),
                    )
                    .await
                    .unwrap_err();
                    assert!(matches!(
                        error, KnowledgeSemanticError::Data(ref cause)
                        if matches!(cause.as_ref(), KnowledgeEmbeddingError::Index(
                            KnowledgeIndexError::LimitExceeded("identifier_bytes")
                        ))
                    ));
                    assert!(error.source().is_some());
                    assert_eq!(connected.load(Ordering::SeqCst), 0);
                    assert_eq!(polled.load(Ordering::SeqCst), 0);
                    limits.content.max_identifier_bytes = n(slug.len());
                    let error = query_knowledge_semantics(
                        ctx.snapshot(),
                        &read,
                        &model.pipeline(&EmbeddingMetadata::local(model.expected).unwrap()),
                        "Paris",
                        query_options(),
                        limits,
                        |operation| model.infer(operation, (failed_connect)()),
                    )
                    .await
                    .unwrap_err();
                    assert!(matches!(
                        error, KnowledgeSemanticError::Models(ref cause)
                        if cause.kind() == ModelErrorKind::Transport
                    ));
                    assert_eq!(
                        error
                            .source()
                            .unwrap()
                            .source()
                            .unwrap()
                            .downcast_ref::<io::Error>()
                            .unwrap()
                            .kind(),
                        io::ErrorKind::ConnectionRefused,
                    );
                    assert_eq!(connected.load(Ordering::SeqCst), 1);
                    assert_eq!(polled.load(Ordering::SeqCst), 1);
                    assert!(capture(&ctx, &read).await.semantic_generation().is_none());
                });
        })
        .unwrap()
        .join()
        .unwrap();
}
#[allow(
    dead_code,
    reason = "existing shared HTTP fixture also covers rerank clients"
)]
#[path = "../../graphit-ai/tests/support/rerank_http.rs"]
mod remote_http;

struct RemoteInference<'a> {
    issuer: &'a RoleIssuer,
    runtime: &'a RuntimeKeyOwner,
    metadata: &'a EmbeddingMetadata,
    resources: &'a graphit_app::embedding_service::EmbeddingResources<'a>,
    original: &'a RequestContext<'a>,
    calls: &'a AtomicUsize,
}
impl RemoteInference<'_> {
    async fn infer(
        &self,
        operation: EmbedOperation,
    ) -> Result<Vec<Vec<f32>>, graphit_app::embedding_service::EmbeddingServiceError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let request = RequestContext::new(
            self.original.content_limits(),
            self.original.cancellation(),
            operation.deadline,
        )
        .map_err(io::Error::other)?;
        let key = self
            .runtime
            .check(request.snapshot())
            .map_err(io::Error::other)?;
        let raw = std::str::from_utf8(key.secret(request.snapshot()).map_err(io::Error::other)?)
            .map_err(io::Error::other)?;
        let caller = self
            .issuer
            .authenticate_runtime(raw, &request)
            .map_err(io::Error::other)?;
        let mut selection = self
            .issuer
            .capture_service(&caller, ServicePurpose::Embedding)?;
        let result = graphit_app::embedding_service::embed(
            &mut selection,
            self.resources,
            self.metadata,
            operation,
        )
        .await?;
        assert!(result.metadata.local_generation.is_none());
        Ok(result.vectors)
    }
}

#[test]
fn actual_direct_embedding_stream_generation_attach_skip_limits_and_cancel() {
    std::thread::Builder::new()
        .name("knowledge-direct-model".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(actual_direct_embedding_stream_scenario());
        })
        .unwrap()
        .join()
        .unwrap();
}
async fn actual_direct_embedding_stream_scenario() {
    use graphit_auth::broker::{JsonLimits, VerifierLimits};
    use graphit_http::client::{BoundedClient, ClientPolicy, Limits};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let directory = tempfile::tempdir().unwrap();
    let f = Fixture::new(&ctx, directory.path());
    let response = |vectors: Vec<Vec<f32>>| {
        remote_http::reply(serde_json::json!({
            "data":vectors.into_iter().enumerate().map(|(index,embedding)|
                serde_json::json!({"object":"embedding","index":index,"embedding":embedding}))
                .collect::<Vec<_>>()
        }))
    };
    let http_fixture = remote_http::fixture_with_deadline(
        vec![
            response(vec![vec![1., 0., 0., 0.], vec![0., 1., 0., 0.]]),
            response(vec![vec![0.9, 0.1, 0., 0.]]),
            response(vec![vec![0.1, 0.9, 0., 0.]]),
            response(vec![vec![0., 0., 1., 0.]]),
            remote_http::Reply {
                status: 503,
                body: "fixture unavailable".into(),
                delay: Duration::ZERO,
            },
            response(vec![vec![0., 0., 0., 1.]]),
            response(vec![vec![0., 0., 0.5, 0.5]]),
        ],
        ctx.snapshot().required_deadline().unwrap(),
    )
    .await;
    let issuer = f.issuer_with_direct(&ctx, Some(&http_fixture.origin), "fixture-weights-r1");
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue(&issuer, &parent, &permissions());
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let network = Limits {
        request_body_bytes: 65536,
        response_body_bytes: 65536,
        url_bytes: 4096,
        header_bytes: 4096,
        header_count: 32,
        max_in_flight: 1,
        max_idle_per_host: 1,
        idle_timeout: Duration::from_secs(30),
        max_certificates: 8,
        max_certificate_bytes: 65536,
    };
    let http = BoundedClient::new(
        ClientPolicy {
            origin: &http_fixture.origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network,
    )
    .unwrap();
    let brokers = graphit_daemon::broker_verifiers::BrokerVerifierCache::new(
        network,
        VerifierLimits {
            token_bytes: 4096,
            json: JsonLimits {
                bytes: 65536,
                nodes: 4096,
                depth: 32,
                scalar_bytes: 4096,
                collection_entries: 128,
            },
            max_keys: 8,
            metadata_ttl: Duration::from_secs(30),
            key_ttl: Duration::from_secs(30),
            unknown_key_cooldown: Duration::from_secs(1),
        },
        true,
        None,
        &[],
    );
    let resources = graphit_app::embedding_service::EmbeddingResources {
        local: None,
        http: Some(&http),
        brokers: &brokers,
        limits: graphit_ai::embedding::EmbeddingLimits::new(32, 65536, 4, 128, 32, 65536, 65536)
            .unwrap(),
    };
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let metadata = graphit_app::embedding_service::metadata(&mut selection, &resources)
        .await
        .unwrap();
    assert_eq!(metadata.dimensions, 4);
    let mut rpc = ChildOptions::standard().unwrap().rpc;
    rpc.max_texts = 1;
    let model = KnowledgeSemanticModel {
        metadata: &metadata,
        batch_limits: rpc,
        rpc_timeout: Duration::from_secs(30),
    };
    let connections = AtomicUsize::new(0);
    let direct = RemoteInference {
        issuer: &issuer,
        runtime: &f.runtime,
        metadata: &metadata,
        resources: &resources,
        original: &ctx,
        calls: &connections,
    };
    let sources = ["docs/one.md", "docs/two.md"].map(|path| {
        KnowledgeSourceIdentity::from_relative_path(Path::new(path), None, n(256)).unwrap()
    });
    let mut fields = [page("one", &[]), page("two", &[])];
    for (page, source) in fields.iter_mut().zip(&sources) {
        page.source_path = source.source_path();
    }
    let prepared = h::prepare(ctx.snapshot(), &fields, &[], hlimits()).unwrap();
    let documents = publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits())
        .await
        .unwrap();
    drop(prepared);
    let mut source_generation = documents.default;
    // The initial authored publication contains two rows in one real Source
    // batch, so this explicitly exercises a two-input provider RPC.
    let batch_rpc = ChildOptions::standard().unwrap().rpc;
    assert!(batch_rpc.max_texts > 1);
    let batch_model = KnowledgeSemanticModel {
        batch_limits: batch_rpc,
        ..model
    };
    let mut initial_limits = index_limits();
    initial_limits.read.batch_rows = NonZeroU32::new(2).unwrap();
    let first = index_knowledge_semantics_at_generation(
        ctx.snapshot(),
        &read,
        &write,
        &batch_model,
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        initial_limits,
        |operation| direct.infer(operation),
        &source_generation,
    )
    .await
    .unwrap();
    assert_eq!(connections.load(Ordering::SeqCst), 1);
    let KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Committed {
        generation: first_semantic,
        ..
    }) = first
    else {
        panic!("actual model sidecar publication required")
    };
    fields[0].body = Some("fresh batched one");
    fields[1].body = Some("fresh batched two");
    source_generation = reconcile_pages(&f, &ctx, &read, &write, &fields).await;
    let mut batch_limits = index_limits();
    batch_limits.read.batch_rows = NonZeroU32::new(2).unwrap();
    connections.store(0, Ordering::SeqCst);
    // FILE-scoped commits may yield separate SDK batches. Request one input
    // explicitly here, preserving the scalar protocol proof on either layout.
    let batched = index_knowledge_semantics_at_generation(
        ctx.snapshot(),
        &read,
        &write,
        &model,
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        batch_limits,
        |operation| direct.infer(operation),
        &source_generation,
    )
    .await
    .unwrap();
    let KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Committed {
        generation: batched_semantic,
        ..
    }) = batched
    else {
        panic!("fresh FILE inputs require a confirmed publication")
    };
    assert_eq!(connections.load(Ordering::SeqCst), 2);
    assert_ne!(batched_semantic, first_semantic);
    let KnowledgeEmbeddingRead::Published(batched_capture) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("batched inference must preserve source publication")
    };
    assert!(batched_capture.source_generation().version > source_generation.version);
    assert_eq!(
        batched_capture.source_generation().version,
        batched_semantic.version
    );
    source_generation = batched_capture.source_generation().clone();
    assert_eq!(
        batched_capture.semantic_generation(),
        Some(batched_semantic.clone())
    );
    connections.store(0, Ordering::SeqCst);
    for mode in [
        KnowledgeEmbeddingWriteMode::Attach,
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
    ] {
        let mut batch_limits = index_limits();
        batch_limits.read.batch_rows = NonZeroU32::new(2).unwrap();
        let repeat = index_knowledge_semantics_at_generation(
            ctx.snapshot(),
            &read,
            &write,
            &model,
            mode,
            batch_limits,
            |operation| direct.infer(operation),
            &source_generation,
        )
        .await
        .unwrap();
        assert!(matches!(
            repeat,
            KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Unchanged)
        ));
    }
    assert_eq!(connections.load(Ordering::SeqCst), 0);
    let mut wrong = source_generation.clone();
    wrong.version += 1;
    assert!(
        matches!(index_knowledge_semantics_at_generation(ctx.snapshot(),
&read,
&write,
&model,
KnowledgeEmbeddingWriteMode::ReplaceComplete,
index_limits(),
|operation| direct.infer(operation),
&wrong).await,
            Err(KnowledgeSemanticError::Data(cause)) if matches!(*cause, KnowledgeEmbeddingError::Conflict))
    );
    assert_eq!(connections.load(Ordering::SeqCst), 0);

    fields[1].body = Some("changed second input; first input is reusable");
    source_generation = reconcile_pages(&f, &ctx, &read, &write, &fields).await;
    let mut mixed_limits = index_limits();
    mixed_limits.read.batch_rows = NonZeroU32::new(2).unwrap();
    let mixed = index_knowledge_semantics_at_generation(
        ctx.snapshot(),
        &read,
        &write,
        &model,
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        mixed_limits,
        |operation| direct.infer(operation),
        &source_generation,
    )
    .await
    .unwrap();
    assert_eq!(connections.load(Ordering::SeqCst), 1);
    let KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Committed {
        generation: mixed_semantic,
        ..
    }) = mixed
    else {
        panic!("one fresh input must publish with its cached neighbor")
    };
    assert_ne!(mixed_semantic, batched_semantic);
    let mixed_capture = capture(&ctx, &read).await;
    assert!(mixed_capture.source_generation().version > source_generation.version);
    assert_eq!(
        mixed_capture.source_generation().version,
        mixed_semantic.version
    );
    assert_eq!(
        mixed_capture.semantic_generation(),
        Some(mixed_semantic.clone())
    );
    assert_eq!(mixed_capture.stored_inputs().count(), 2);
    let previous = batched_capture
        .stored_inputs()
        .find(|(i, _)| i.slug == "one")
        .unwrap();
    let reused = mixed_capture
        .stored_inputs()
        .find(|(i, _)| i.slug == "one")
        .unwrap();
    assert_eq!(previous.0.input_fingerprint, reused.0.input_fingerprint);
    assert_eq!(previous.1, reused.1);

    source_generation = reconcile_pages(&f, &ctx, &read, &write, &fields[..1]).await;
    connections.store(0, Ordering::SeqCst);
    let pruned = index_knowledge_semantics_at_generation(
        ctx.snapshot(),
        &read,
        &write,
        &model,
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        index_limits(),
        |operation| direct.infer(operation),
        &source_generation,
    )
    .await
    .unwrap();
    assert_eq!(connections.load(Ordering::SeqCst), 0);
    assert!(matches!(
        pruned,
        KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Unchanged)
    ));
    let surviving = capture(&ctx, &read).await;
    assert_eq!(surviving.source_generation(), &source_generation);
    let pruned_semantic = surviving.semantic_generation().unwrap();
    assert_eq!(pruned_semantic.version, source_generation.version);
    assert_ne!(pruned_semantic, mixed_semantic);
    assert_eq!(
        surviving
            .stored_inputs()
            .map(|(i, _)| i.slug)
            .collect::<Vec<_>>(),
        ["one"]
    );

    // The cached first batch is followed by an actual HTTP 503 for the fresh
    // input. A failed remote callback is never treated as a healthy EOF.
    source_generation = reconcile_pages(&f, &ctx, &read, &write, &fields).await;
    let late_attempts = AtomicUsize::new(0);
    let late = index_knowledge_semantics_at_generation(
        ctx.snapshot(),
        &read,
        &write,
        &model,
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        index_limits(),
        |operation| {
            late_attempts.fetch_add(1, Ordering::SeqCst);
            direct.infer(operation)
        },
        &source_generation,
    )
    .await
    .unwrap_err();
    assert_eq!(late_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(connections.load(Ordering::SeqCst), 1);
    connections.store(0, Ordering::SeqCst);
    assert!(matches!(late, KnowledgeSemanticError::Inference(ref cause)
            if matches!(cause.downcast_ref::<graphit_app::embedding_service::EmbeddingServiceError>(),
                Some(graphit_app::embedding_service::EmbeddingServiceError::Direct(_)))));
    let after_failure = capture(&ctx, &read).await;
    assert_eq!(after_failure.source_generation(), &source_generation);
    assert_eq!(
        after_failure.semantic_generation().unwrap().version,
        source_generation.version
    );
    assert_eq!(
        after_failure
            .stored_inputs()
            .map(|(i, _)| i.slug)
            .collect::<Vec<_>>(),
        ["one"]
    );

    let tiny_resources = graphit_app::embedding_service::EmbeddingResources {
        limits: graphit_ai::embedding::EmbeddingLimits::new(32, 1, 4, 128, 32, 65536, 65536)
            .unwrap(),
        ..resources
    };
    let tiny_calls = AtomicUsize::new(0);
    let tiny = RemoteInference {
        resources: &tiny_resources,
        calls: &tiny_calls,
        ..direct
    };
    let limited = KnowledgeSemanticModel {
        batch_limits: rpc,
        ..model
    };
    assert!(
        matches!(index_knowledge_semantics_at_generation(ctx.snapshot(),
&read,
&write,
&limited,
KnowledgeEmbeddingWriteMode::ReplaceComplete,
index_limits(),
|operation| tiny.infer(operation),
&source_generation).await, Err(KnowledgeSemanticError::Inference(ref cause)) if matches!(
                cause.downcast_ref::<graphit_app::embedding_service::EmbeddingServiceError>(),
                Some(graphit_app::embedding_service::EmbeddingServiceError::Direct(
                    graphit_ai::direct_embedding::DirectEmbeddingError::Embedding {
                        source: graphit_ai::embedding::EmbeddingError::Limit, .. }))))
    );
    assert_eq!(connections.load(Ordering::SeqCst), 0);
    let KnowledgeEmbeddingRead::Published(captured) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("document effect must remain")
    };
    assert_eq!(captured.source_generation(), &source_generation);
    assert_eq!(
        captured.semantic_generation().unwrap().version,
        source_generation.version
    );

    // Changing a configured provider does not silently widen the issuer's fixed
    // service policy. Drain old leases, preserve the confirmed Source delta and
    // recreate the actual issuer with the new binding, as after owner restart.
    drop(selection);
    fields[1].body = Some("Source delta confirmed before the provider is restarted");
    source_generation = reconcile_pages(&f, &ctx, &read, &write, &fields).await;
    let unchanged_one = capture(&ctx, &read)
        .await
        .read_selected_documents(ctx.snapshot(), &read, &["one"])
        .await
        .unwrap();
    let store = f.profile_store(&ctx);
    let mutation = store
        .mutate(store.load(&ctx).unwrap(), &ctx, |document, original| {
            let provider = document.provider("service").unwrap();
            let previous = provider.revision();
            let mut config = provider.config().clone();
            let AiService::Direct { model_revision, .. } = &mut config.embedding else {
                panic!("Direct fixture")
            };
            *model_revision = "fixture-weights-r2".into();
            document.update_provider(
                "service",
                config,
                document.revision(),
                previous,
                2,
                original,
            )?;
            let expectation = ProfileExpectation {
                document_revision: document.revision(),
                provider_revision: document.provider("service").unwrap().revision(),
                profile_revision: Some(document.profile("fixture").unwrap().revision()),
            };
            document.replace_profile_data(
                ProfileData {
                    name: "fixture".into(),
                    provider: "service".into(),
                    username: "fixture".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new(SecretString::from("fixture-key-only"))),
                        broker_key: None,
                        embedding_api_key: Some(Secret::new(SecretString::from(
                            "fixture-remote-embedding-key",
                        ))),
                        rerank_api_key: None,
                        s3: None,
                    }),
                },
                expectation,
                2,
                original,
            )?;
            document.activate("fixture", document.revision(), original)?;
            Ok(())
        })
        .unwrap();
    assert!(mutation.failure().is_none());
    assert!(mutation.cleanup_failure().is_none());
    let old_caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(matches!(
        issuer.capture_service(&old_caller, ServicePurpose::Embedding),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    assert_eq!(
        capture(&ctx, &read).await.source_generation(),
        &source_generation,
        "a denied model request cannot roll back confirmed Source"
    );

    let refreshed = f.issuer_with_direct(&ctx, Some(&http_fixture.origin), "fixture-weights-r2");
    let fresh_caller = refreshed.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&refreshed);
    let fresh_read = authority
        .admit_authenticated_read(&fresh_caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
        .unwrap();
    let fresh_write = authority
        .admit_authenticated_write(
            &fresh_caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::Knowledge,
        )
        .unwrap();
    let mut fresh_service = refreshed
        .capture_service(&fresh_caller, ServicePurpose::Embedding)
        .unwrap();
    let fresh_metadata = graphit_app::embedding_service::metadata(&mut fresh_service, &resources)
        .await
        .unwrap();
    assert_eq!(fresh_metadata.revision, "fixture-weights-r2");
    assert_ne!(fresh_metadata.vector_id, metadata.vector_id);
    let fresh_calls = AtomicUsize::new(0);
    let fresh_inference = RemoteInference {
        issuer: &refreshed,
        runtime: &f.runtime,
        metadata: &fresh_metadata,
        resources: &resources,
        original: &ctx,
        calls: &fresh_calls,
    };
    let fresh_model = KnowledgeSemanticModel {
        metadata: &fresh_metadata,
        batch_limits: rpc,
        rpc_timeout: Duration::from_secs(30),
    };
    let selected = std::collections::BTreeSet::from(["one".to_owned(), "two".to_owned()]);
    let replaced = index_knowledge_semantics_selection(
        ctx.snapshot(),
        &fresh_read,
        &fresh_write,
        &fresh_model,
        index_limits(),
        |operation| fresh_inference.infer(operation),
        &selected,
    )
    .await
    .unwrap();
    assert!(matches!(
        replaced,
        KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Committed { .. })
    ));
    assert_eq!(
        fresh_calls.load(Ordering::SeqCst),
        2,
        "both documents need the new vector space"
    );
    let current = capture(&ctx, &fresh_read).await;
    assert!(current.source_generation().version > source_generation.version);
    assert_eq!(
        current.source_generation().version,
        current.semantic_generation().unwrap().version
    );
    source_generation = current.source_generation().clone();
    assert_eq!(
        current
            .stored_inputs()
            .map(|(input, identity)| {
                assert_eq!(
                    identity.vector_id(),
                    fresh_metadata.vector_id,
                    "old and new spaces cannot be mixed"
                );
                input.slug
            })
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["one", "two"])
    );
    let one = current
        .read_selected_documents(ctx.snapshot(), &fresh_read, &["one"])
        .await
        .unwrap();
    assert_eq!(
        one.documents, unchanged_one.documents,
        "model replacement does not rewrite intact Source"
    );
    assert_eq!(one.state, unchanged_one.state);
    fresh_calls.store(0, Ordering::SeqCst);
    let repeated = index_knowledge_semantics_selection(
        ctx.snapshot(),
        &fresh_read,
        &fresh_write,
        &fresh_model,
        index_limits(),
        |operation| fresh_inference.infer(operation),
        &selected,
    )
    .await
    .unwrap();
    assert!(matches!(
        repeated,
        KnowledgeSemanticIndexResult::Published(KnowledgeEmbeddingPublication::Unchanged)
    ));
    assert_eq!(fresh_calls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        issuer.capture_service(&old_caller, ServicePurpose::Embedding),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));

    cancel.cancel();
    assert!(
        matches!(index_knowledge_semantics_at_generation(ctx.snapshot(),
&read,
&write,
&model,
KnowledgeEmbeddingWriteMode::ReplaceComplete,
index_limits(),
|operation| direct.infer(operation),
&source_generation).await,
            Err(KnowledgeSemanticError::Request(cause)) if cause.kind() == io::ErrorKind::Interrupted)
    );
    assert_eq!(connections.load(Ordering::SeqCst), 0);
    let observed = http_fixture.server.await.unwrap();
    assert_eq!(observed.len(), 7);
    assert_eq!(
        observed
            .iter()
            .map(|request| request.body["input"].as_array().unwrap().len())
            .collect::<Vec<_>>(),
        [2, 1, 1, 1, 1, 1, 1]
    );
    for request in observed {
        assert!(request.head.contains("Bearer fixture-remote-embedding-key"));
        assert_eq!(request.body["model"], "fixture-embedding");
    }
}

#[test]
fn null_vector_with_retained_identity_is_embedded_once_and_then_reused() {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    use arrow_array::{FixedSizeListArray, Float32Array};
                    use graphit_core::artifact_store::ArtifactStore;
                    use graphit_data::lance_branch_store::BranchSnapshot;

                    let cancel = Cancellation::default();
                    let ctx = request(&cancel);
                    let directory = tempfile::tempdir().unwrap();
                    let f = Fixture::new(&ctx, directory.path());
                    let issuer = f.issuer(&ctx);
                    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
                    let admin = issuer.bootstrap_admin(&caller).unwrap();
                    let parent = issuer.admin_parent(&caller, &admin).unwrap();
                    let (bearer, reference) = f.issue(&issuer, &parent, &permissions());
                    let grant = issuer
                        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
                        .unwrap();
                    let authority = ProjectRecordAuthority::new(&issuer);
                    let read = authority
                        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
                        .unwrap();
                    let write = authority
                        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
                        .unwrap();
                    publish_pages(&ctx, &write, &[page("a", &[])]).await;
                    let metadata = EmbeddingMetadata::remote(
                        [7; 32],
                        2,
                        "fixture-model".into(),
                        "weights-r1".into(),
                    )
                    .unwrap();
                    let model = KnowledgeSemanticModel {
                        metadata: &metadata,
                        batch_limits: RpcLimits::new(65536, 2, 32768, 4).unwrap(),
                        rpc_timeout: Duration::from_secs(10),
                    };
                    let calls = AtomicUsize::new(0);
                    let selected = std::collections::BTreeSet::from(["a".to_owned()]);
                    index_knowledge_semantics_selection(
                        ctx.snapshot(),
                        &read,
                        &write,
                        &model,
                        index_limits(),
                        |operation| {
                            assert!(matches!(operation.input, EmbedInput::Document));
                            assert_eq!(operation.texts.len(), 1);
                            calls.fetch_add(1, Ordering::SeqCst);
                            std::future::ready(Ok::<_, io::Error>(vec![vec![1., 0.]]))
                        },
                        &selected,
                    )
                    .await
                    .unwrap();
                    assert_eq!(calls.load(Ordering::SeqCst), 1);
                    let before = capture(&ctx, &read).await;
                    let identity = before.stored_inputs().next().unwrap().1.clone();
                    let input_fingerprint =
                        before.stored_inputs().next().unwrap().0.input_fingerprint;
                    let documents = before
                        .read_selected_documents(ctx.snapshot(), &read, &["a"])
                        .await
                        .unwrap()
                        .documents;
                    let uri = &before.source_generation().corpus_uri;
                    let store = ArtifactStore::new(f.project.clone(), write_limits().artifacts);
                    let writer = store.writer_with_context(ctx.snapshot()).unwrap();
                    let current = BranchSnapshot::capture_observed(uri, None, None)
                        .await
                        .unwrap();
                    let vector_field = current.schema().project(&[22]).unwrap();
                    let restored = current
                        .drop_vector("vector", ctx.snapshot())
                        .await
                        .unwrap()
                        .add_nullable_columns(Arc::new(vector_field), ctx.snapshot())
                        .await
                        .unwrap();
                    drop(writer);
                    let pending = capture(&ctx, &read).await;
                    assert_eq!(pending.source_generation().version, restored.version());
                    assert_eq!(
                        pending.stored_inputs().count(),
                        0,
                        "NULL is pending even when identity fields remain"
                    );
                    assert_eq!(
                        pending
                            .read_selected_documents(ctx.snapshot(), &read, &["a"])
                            .await
                            .unwrap()
                            .documents,
                        documents
                    );

                    calls.store(0, Ordering::SeqCst);
                    let filled = index_knowledge_semantics_selection(
                        ctx.snapshot(),
                        &read,
                        &write,
                        &model,
                        index_limits(),
                        |operation| {
                            assert!(matches!(operation.input, EmbedInput::Document));
                            assert_eq!(operation.texts.len(), 1);
                            calls.fetch_add(1, Ordering::SeqCst);
                            std::future::ready(Ok::<_, io::Error>(vec![vec![0., 1.]]))
                        },
                        &selected,
                    )
                    .await
                    .expect("an absent vector must accept one inferred value, not AlreadyEmbedded");
                    assert!(matches!(
                        filled,
                        KnowledgeSemanticIndexResult::Published(
                            KnowledgeEmbeddingPublication::Committed { .. }
                        )
                    ));
                    assert_eq!(calls.load(Ordering::SeqCst), 1);
                    let filled = capture(&ctx, &read).await;
                    assert_eq!(filled.stored_inputs().count(), 1);
                    let inputs = [KnowledgeEmbeddingInput {
                        slug: "a",
                        input_fingerprint,
                    }];
                    let vectors = filled
                        .read_reusable_embedding_batch(ctx.snapshot(), &read, &identity, &inputs)
                        .await
                        .unwrap();
                    let vector = vectors
                        .column(5)
                        .as_any()
                        .downcast_ref::<FixedSizeListArray>()
                        .unwrap()
                        .value(0);
                    assert_eq!(
                        vector
                            .as_any()
                            .downcast_ref::<Float32Array>()
                            .unwrap()
                            .values()
                            .as_ref(),
                        &[0., 1.]
                    );
                    let generation = filled.semantic_generation();
                    calls.store(0, Ordering::SeqCst);
                    let repeated = index_knowledge_semantics_selection(
                        ctx.snapshot(),
                        &read,
                        &write,
                        &model,
                        index_limits(),
                        |_| {
                            calls.fetch_add(1, Ordering::SeqCst);
                            std::future::ready(Err::<Vec<Vec<f32>>, _>(io::Error::other(
                                "intact vector was reinferred",
                            )))
                        },
                        &selected,
                    )
                    .await
                    .unwrap();
                    assert!(matches!(
                        repeated,
                        KnowledgeSemanticIndexResult::Published(
                            KnowledgeEmbeddingPublication::Unchanged
                        )
                    ));
                    assert_eq!(calls.load(Ordering::SeqCst), 0);
                    assert_eq!(capture(&ctx, &read).await.semantic_generation(), generation);
                });
        })
        .unwrap()
        .join()
        .unwrap();
}
