//! Real public Auth producers for the Direct AI integration suites.
use graphit_auth::{
    AdminSessionPolicy, CredentialSource, ExternalEffect, IssuerAuthorityPolicy, IssuerLimits,
    Permission, RemoteServiceBindingInput, RoleIssuer, ScopeCapture, ServiceAdmissionLimits,
    ServiceAuthorityInput, ServiceOrigin, ServicePurpose, ServiceRuleInput, SourceAuthorityPolicy,
    TtlPolicy,
    profile::{
        AiService, BrokerService, DirectProtocol, LocalCredentials, ProfileCredentials,
        ProfileData, ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location,
        S3Source, Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeBindingRecord, ScopeLimits},
    auth_document::{
        AuthDocumentLimits, AuthDocumentStore,
        publication::{AuthDocumentPublication, AuthPublicationEffect},
    },
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    project_paths::InstallationPaths,
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use secrecy::SecretString;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) const MCP_KEY: &str = "private-fixture-mcp-key";
pub(super) const EMBEDDING_KEY: &str = "private-fixture-embedding-key";
pub(super) const RERANK_KEY: &str = "private-fixture-rerank-key";
const MODEL: &str = "fixture-native-model";
const EMBEDDING_MODEL: &str = "fixture-embedding-model";
const MODEL_REVISION: &str = "fixture-model-v1";
const EMBEDDING_RECIPE: &str = "fixture-embedding-recipe-v1";
const RERANK_RECIPE: &str = "fixture-rerank-recipe-v1";

pub(super) fn request(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 4096,
            max_output_bytes: 4096,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 256,
            max_collection_entries: 64,
        },
        cancel,
        deadline,
    )
    .unwrap()
}
#[track_caller]
fn applied(result: AuthDocumentPublication) {
    assert_eq!(
        (result.effect(), result.failure(), result.cleanup_failure()),
        (AuthPublicationEffect::Applied, None, None),
        "Auth publication cleanup effect: {:?}",
        result.cleanup_effect(),
    );
}
fn profile(key: &str) -> ProfileData {
    ProfileData {
        name: "fixture".into(),
        provider: "service".into(),
        username: "fixture".into(),
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Local(LocalCredentials {
            anonymous: false,
            mcp_key: Some(Secret::new(SecretString::from(MCP_KEY))),
            broker_key: Some(Secret::new(SecretString::from(
                "private-fixture-broker-key",
            ))),
            embedding_api_key: Some(Secret::new(SecretString::from(EMBEDDING_KEY))),
            rerank_api_key: Some(Secret::new(SecretString::from(key))),
            s3: None,
        }),
    }
}
pub(super) struct EmbeddingConfig<'a> {
    pub(super) protocol: DirectProtocol,
    pub(super) endpoint: &'a str,
    pub(super) model: &'a str,
    pub(super) dimensions: u32,
    pub(super) request_dimensions: Option<u32>,
    pub(super) batch_endpoint: Option<&'a str>,
}
pub(super) struct AuthFixture {
    runtime: Arc<RuntimeKeyOwner>,
    installation: CapturedPublicationScope,
    record: ScopeBindingRecord,
    pub(super) store: Arc<AuthStore>,
    endpoint: String,
    protocol: Option<DirectProtocol>,
    embedding_protocol: Option<DirectProtocol>,
    embedding_model: String,
    embedding_dimensions: u32,
    request_dimensions: Option<u32>,
    batch_endpoint: Option<String>,
    _directory: tempfile::TempDir,
}
impl AuthFixture {
    pub(super) fn new(
        request: &RequestContext<'_>,
        protocol: DirectProtocol,
        endpoint: &str,
    ) -> Self {
        Self::build(
            request,
            Some(protocol),
            Some(DirectProtocol::OpenAi),
            endpoint,
            None,
        )
    }
    pub(super) fn broker(request: &RequestContext<'_>, endpoint: &str) -> Self {
        Self::build(request, None, None, endpoint, None)
    }
    #[allow(
        dead_code,
        reason = "shared fixture constructor used by the embedding integration target"
    )]
    pub(super) fn embedding(
        request: &RequestContext<'_>,
        protocol: DirectProtocol,
        endpoint: &str,
    ) -> Self {
        Self::build(
            request,
            Some(DirectProtocol::Cohere),
            Some(protocol),
            endpoint,
            None,
        )
    }
    #[allow(
        dead_code,
        reason = "shared fixture options are used by the embedding integration target"
    )]
    pub(super) fn embedding_config(
        request: &RequestContext<'_>,
        config: EmbeddingConfig<'_>,
    ) -> Self {
        Self::build(
            request,
            Some(DirectProtocol::Cohere),
            Some(config.protocol),
            config.endpoint,
            Some(&config),
        )
    }
    fn build(
        request: &RequestContext<'_>,
        protocol: Option<DirectProtocol>,
        embedding_protocol: Option<DirectProtocol>,
        endpoint: &str,
        embedding: Option<&EmbeddingConfig<'_>>,
    ) -> Self {
        let embedding_model = embedding.map_or(EMBEDDING_MODEL, |config| config.model);
        let embedding_dimensions = embedding.map_or(3, |config| config.dimensions);
        let request_dimensions = embedding.and_then(|config| config.request_dimensions);
        let batch_endpoint = embedding.and_then(|config| config.batch_endpoint);
        let directory = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(directory.path()).unwrap();
        let home = base.join("home");
        let root = base.join("project");
        std::fs::create_dir(&home).unwrap();
        std::fs::create_dir(&root).unwrap();
        let paths = InstallationPaths::resolve(Brand::default(), &root, Some(&home), None).unwrap();
        let private = installation_bootstrap::prepare(
            &paths,
            BootstrapLimits::new(65536, 256).unwrap(),
            request.snapshot(),
        )
        .into_installation()
        .unwrap();
        let runtime = Arc::new(
            RuntimeKeyOwner::start(
                private,
                RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
                request.snapshot(),
            )
            .into_owner()
            .ok()
            .unwrap(),
        );
        let id = InstallationId::new("local-installation").unwrap();
        let scopes = ScopeLimits::new(16, 4096, 128).unwrap();
        let installation = CapturedPublicationScope::installation(
            &paths,
            id.clone(),
            [root],
            scopes,
            request.snapshot(),
        )
        .unwrap();
        let record = installation.binding().unwrap();
        let store_scope = CapturedPublicationScope::installation(
            &paths,
            id,
            [installation.owner_root().to_owned()],
            scopes,
            request.snapshot(),
        )
        .unwrap();
        let physical = AuthDocumentStore::capture(
            store_scope,
            AuthDocumentLimits::new(4096, 65536, 256, 65536).unwrap(),
            request.snapshot(),
        )
        .unwrap();
        let store = Arc::new(
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
                request,
            )
            .unwrap(),
        );
        let broker = protocol.is_none().then(|| BrokerService {
            endpoint: endpoint.into(),
            mcp_resource: None,
            allow_anonymous: false,
        });
        let config = ProviderConfig {
            kind: ProviderKind::Local {
                allow_aws_credential_chain: false,
                broker,
            },
            embedding: embedding_protocol.map_or(AiService::Broker, |protocol| AiService::Direct {
                protocol,
                endpoint: endpoint.into(),
                model: embedding_model.into(),
                model_revision: MODEL_REVISION.into(),
                recipe: EMBEDDING_RECIPE.into(),
                dimensions: embedding_dimensions,
                request_dimensions,
                batch_endpoint: batch_endpoint.map(str::to_owned),
            }),
            rerank: protocol.map_or(AiService::Broker, |protocol| AiService::Direct {
                protocol,
                endpoint: endpoint.into(),
                model: MODEL.into(),
                model_revision: MODEL_REVISION.into(),
                recipe: RERANK_RECIPE.into(),
                dimensions: 0,
                request_dimensions: None,
                batch_endpoint: None,
            }),
            s3: S3Location {
                bucket: None,
                region: None,
                endpoint: None,
                prefix: None,
                source: S3Source::LocalCredentials,
            },
        };
        let before = store.load(request).unwrap();
        applied(
            store
                .mutate(before, request, |doc, ctx| {
                    doc.add_provider("service".into(), config, doc.revision(), 1, ctx)?;
                    let expected = ProfileExpectation {
                        document_revision: doc.revision(),
                        provider_revision: doc.provider("service").unwrap().revision(),
                        profile_revision: None,
                    };
                    doc.replace_profile_data(profile(RERANK_KEY), expected, 1, ctx)?;
                    doc.activate("fixture", doc.revision(), ctx)?;
                    Ok(())
                })
                .unwrap(),
        );
        Self {
            runtime,
            installation,
            record,
            store,
            endpoint: endpoint.into(),
            protocol,
            embedding_protocol,
            embedding_model: embedding_model.into(),
            embedding_dimensions,
            request_dimensions,
            batch_endpoint: batch_endpoint.map(str::to_owned),
            _directory: directory,
        }
    }
    pub(super) fn issuer(&self, ctx: &RequestContext<'_>, purpose: ServicePurpose) -> RoleIssuer {
        let selected_protocol = match purpose {
            ServicePurpose::Embedding => self.embedding_protocol,
            _ => self.protocol,
        };
        let binding = match (purpose, selected_protocol) {
            (
                ServicePurpose::RecordStorage
                | ServicePurpose::InventoryRead
                | ServicePurpose::InventoryPublish
                | ServicePurpose::HubRead
                | ServicePurpose::HubWrite
                | ServicePurpose::PresenceRead
                | ServicePurpose::PresencePublish,
                _,
            ) => panic!("non-AI services are not part of the AI fixture"),
            (ServicePurpose::Embedding, Some(protocol)) => RemoteServiceBindingInput::Direct {
                provider: "service",
                purpose,
                protocol,
                endpoint: &self.endpoint,
                model: &self.embedding_model,
                model_revision: MODEL_REVISION,
                recipe: EMBEDDING_RECIPE,
                dimensions: self.embedding_dimensions,
                request_dimensions: self.request_dimensions,
                batch_endpoint: self.batch_endpoint.as_deref(),
            },
            (ServicePurpose::Rerank, Some(protocol)) => RemoteServiceBindingInput::Direct {
                provider: "service",
                purpose,
                protocol,
                endpoint: &self.endpoint,
                model: MODEL,
                model_revision: MODEL_REVISION,
                recipe: RERANK_RECIPE,
                dimensions: 0,
                request_dimensions: None,
                batch_endpoint: None,
            },
            (ServicePurpose::Embedding | ServicePurpose::Rerank, None) => {
                RemoteServiceBindingInput::Broker {
                    provider: "service",
                    purpose,
                    endpoint: &self.endpoint,
                    resource: None,
                }
            }
        };
        let rules = [ServiceRuleInput::Remote {
            origin: ServiceOrigin::Local,
            binding,
            credential: if selected_protocol.is_none() {
                CredentialSource::LocalBrokerKey
            } else {
                CredentialSource::DirectKey
            },
            incoming: None,
        }];
        let ordinary = [Permission::External(ExternalEffect::RemoteRequest)];
        let administrative = [Permission::ManageAuthProfiles];
        let source = || SourceAuthorityPolicy {
            ordinary: &ordinary,
            administrative: &administrative,
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(3600),
                maximum: Duration::from_secs(3600),
            },
        };
        let policy = IssuerAuthorityPolicy::new(
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
        .unwrap();
        RoleIssuer::new(
            ScopeCapture {
                live: &self.installation,
                record: &self.record,
            },
            Arc::clone(&self.runtime),
            Arc::clone(&self.store),
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
    #[allow(
        dead_code,
        reason = "shared fixture option mutation is used by the embedding target"
    )]
    pub(super) fn change_embedding_options(&self, request: &RequestContext<'_>) {
        let before = self.store.load(request).unwrap();
        applied(
            self.store
                .mutate(before, request, |doc, ctx| {
                    let provider = doc.provider("service").unwrap();
                    let provider_revision = provider.revision();
                    let now = provider.updated_at() + 1;
                    let mut config = provider.config().clone();
                    if let AiService::Direct {
                        protocol,
                        request_dimensions,
                        batch_endpoint,
                        ..
                    } = &mut config.embedding
                    {
                        if *protocol == DirectProtocol::OpenAi {
                            *request_dimensions = Some(3);
                        } else {
                            let route = batch_endpoint.as_mut().unwrap();
                            *route = route.replace("/v1beta/", "/v1/");
                        }
                    } else {
                        panic!("literal Direct options required");
                    }
                    doc.update_provider(
                        "service",
                        config,
                        doc.revision(),
                        provider_revision,
                        now,
                        ctx,
                    )?;
                    Ok(())
                })
                .unwrap(),
        );
    }
    pub(super) fn rotate_key(&self, request: &RequestContext<'_>) {
        let before = self.store.load(request).unwrap();
        applied(
            self.store
                .mutate(before, request, |doc, ctx| {
                    let current = doc.profile("fixture").unwrap();
                    let now = current.updated_at() + 1;
                    let expected = ProfileExpectation {
                        document_revision: doc.revision(),
                        provider_revision: doc.provider("service").unwrap().revision(),
                        profile_revision: Some(current.revision()),
                    };
                    doc.replace_profile_data(
                        profile("private-fixture-rotated-key"),
                        expected,
                        now,
                        ctx,
                    )?;
                    Ok(())
                })
                .unwrap(),
        );
    }
}
