//! Isolated real PROJECT/Auth/service producers for Memory consumer integration.
use super::memory_api::{MemoryStoreReadLimits, MemoryStoreWriteLimits};
use graphit_auth::profile::{
    AiService, LocalCredentials, LocalDevice, LocalExecution, ProfileCredentials, ProfileData,
    ProfileExpectation, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
};
use graphit_auth::*;
use graphit_core::auth_document::publication::AuthPublicationEffect;
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeBindingRecord, ScopeLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::{Checkout, VersionContext},
};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use secrecy::SecretString;
use std::{
    num::{NonZeroU32, NonZeroUsize},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
pub fn request_until(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 1048576,
            max_output_bytes: 1048576,
            max_depth: 64,
            max_nodes: 65536,
            max_scalar_bytes: 65536,
            max_collection_entries: 65536,
        },
        cancel,
        deadline,
    )
    .unwrap()
}
pub fn limits() -> IssuerLimits {
    IssuerLimits {
        max_permissions: 16,
        max_scope_bytes: 65536,
        max_parent_bearer_bytes: 4096,
    }
}
pub fn read_limits() -> MemoryStoreReadLimits {
    MemoryStoreReadLimits {
        max_query_bytes: NonZeroUsize::new(4 * 1024 * 1024).unwrap(),
        native: NativeDocumentFileLimits::new(1048576, 65536, 256, 8 * 1024 * 1024).unwrap(),
        batch_rows: NonZeroU32::new(16).unwrap(),
    }
}
pub fn write_limits() -> MemoryStoreWriteLimits {
    MemoryStoreWriteLimits {
        max_input_bytes: NonZeroUsize::new(1024 * 1024).unwrap(),
        max_row_bytes: NonZeroUsize::new(2 * 1024 * 1024).unwrap(),
        read: read_limits(),
        artifacts: ArtifactLimits::new(
            1048576,
            65536,
            CopyLimits::new(1048576, 128, 65536, 16).unwrap(),
        )
        .unwrap(),
    }
}
pub struct Fixture {
    pub installation: CapturedPublicationScope,
    pub project: CapturedPublicationScope,
    installation_record: ScopeBindingRecord,
    project_record: ScopeBindingRecord,
    pub version: VersionContext,
    pub root: PathBuf,
    runtime: Arc<RuntimeKeyOwner>,
    _temp: tempfile::TempDir,
}
impl Fixture {
    pub fn runtime(&self) -> &RuntimeKeyOwner {
        &self.runtime
    }
    pub fn new(ctx: &RequestContext<'_>, version: &str) -> Self {
        Self::new_for(ctx, version, "01KSH1CRFFG8Z74B5ZS78WW808")
    }
    pub fn new_for(ctx: &RequestContext<'_>, version: &str, project_id: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let root = temp.path().join("project");
        std::fs::create_dir(&home).unwrap();
        std::fs::create_dir(&root).unwrap();
        let install =
            InstallationPaths::resolve(Brand::default(), &root, Some(&home), None).unwrap();
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
        let sl = ScopeLimits::new(16, 4096, 128).unwrap();
        let installation = CapturedPublicationScope::installation(
            &install,
            InstallationId::new("native-memory-test").unwrap(),
            [root.clone()],
            sl,
            ctx.snapshot(),
        )
        .unwrap();
        let project = CapturedPublicationScope::project(
            &paths,
            project_id.parse().unwrap(),
            sl,
            ctx.snapshot(),
        )
        .unwrap();
        let version = VersionContext::select(&paths, Checkout::NonGit, version, None, "").unwrap();
        let installation_record = installation.binding().unwrap();
        let project_record = project.binding().unwrap();
        Self {
            installation,
            project,
            installation_record,
            project_record,
            version,
            root,
            runtime,
            _temp: temp,
        }
    }
    pub fn scope(&self) -> ScopeInput<'_> {
        ScopeInput {
            installation: ScopeCapture {
                live: &self.installation,
                record: &self.installation_record,
            },
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
    pub fn issuer(&self, ctx: &RequestContext<'_>) -> (RoleIssuer, Arc<store::AuthStore>) {
        let paths = InstallationPaths::resolve(
            self.installation.brand().clone(),
            &self.root,
            None,
            Some(self.installation.owner_root()),
        )
        .unwrap();
        let scope = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("native-memory-test").unwrap(),
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
        let store = Arc::new(
            store::AuthStore::new(
                physical,
                profile::ProfileLimits {
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
        );
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
                        embedding: AiService::Local(LocalExecution {
                            device: LocalDevice::Cpu,
                            device_id: 0,
                        }),
                        rerank: AiService::Disabled,
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
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("service").unwrap().revision(),
                    profile_revision: None,
                };
                doc.replace_profile_data(profile("fixture"), expected, 1, ctx)?;
                doc.activate("fixture", doc.revision(), ctx)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(result.effect(), AuthPublicationEffect::Applied);
        assert!(result.failure().is_none());
        assert!(result.cleanup_failure().is_none());
        let permissions = [Permission::Read(Module::Memory), Permission::WriteMemory];
        let sessions = AdminSessionPolicy {
            lifetime: Duration::from_secs(3600),
            maximum: Duration::from_secs(3600),
        };
        let authority = IssuerAuthorityPolicy::new(
            SourceAuthorityPolicy {
                ordinary: &permissions,
                administrative: &permissions,
                sessions,
            },
            SourceAuthorityPolicy {
                ordinary: &permissions,
                administrative: &permissions,
                sessions,
            },
            limits(),
            ServiceAuthorityInput {
                rules: &[ServiceRuleInput::Local {
                    origin: ServiceOrigin::Runtime,
                    provider: "service",
                    purpose: ServicePurpose::Embedding,
                    execution: profile::LocalExecution {
                        device: profile::LocalDevice::Cpu,
                        device_id: 0,
                    },
                }],
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 65536,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap();
        let issuer = RoleIssuer::new(
            self.scope().installation,
            Arc::clone(&self.runtime),
            Arc::clone(&store),
            authority,
            TtlPolicy {
                ttl: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
                refresh_margin: Duration::from_secs(120),
            },
            ctx,
        )
        .unwrap();
        (issuer, store)
    }
}

fn profile(username: &str) -> ProfileData {
    ProfileData {
        name: "fixture".into(),
        provider: "service".into(),
        username: username.into(),
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Local(LocalCredentials {
            anonymous: false,
            mcp_key: Some(Secret::new(SecretString::from("fixture-key-only"))),
            broker_key: None,
            embedding_api_key: None,
            rerank_api_key: None,
            s3: None,
        }),
    }
}

#[allow(
    dead_code,
    reason = "shared auth fixture; revocation is exercised by user_memory_embeddings"
)]
pub fn revoke(store: &store::AuthStore, ctx: &RequestContext<'_>) {
    let before = store.load(ctx).unwrap();
    let result = store
        .mutate(before, ctx, |doc, ctx| {
            let current = doc.profile("fixture").unwrap();
            let now = current.updated_at() + 1;
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("service").unwrap().revision(),
                profile_revision: Some(current.revision()),
            };
            doc.replace_profile_data(profile("changed"), expected, now, ctx)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert!(result.failure().is_none());
    assert!(result.cleanup_failure().is_none());
}

use crate::model_service_revocation as service_revocation;
pub fn revoke_service(store: &store::AuthStore, ctx: &RequestContext<'_>) {
    service_revocation::revoke_service(store, ctx);
}

/// Local SDK execution used by provider-neutral Memory orchestration tests.
/// Connection futures and their fault injection remain owned by each scenario.
pub struct LocalInference<'a, 'lease> {
    pub client: &'a super::model_api::ModelServiceClient,
    pub lease: &'a LocalServiceLease<'lease>,
    pub runtime: &'a RuntimeKeyOwner,
    pub expected: &'a super::model_api::ExpectedModel,
}
impl LocalInference<'_, '_> {
    pub fn identity(&self) -> super::memory_api::MemoryEmbeddingIdentity {
        super::memory_api::MemoryEmbeddingIdentity::new(
            self.expected.vector_id(),
            u32::try_from(self.expected.dimensions().unwrap())
                .ok()
                .and_then(NonZeroU32::new)
                .unwrap(),
            super::memory_api::MEMORY_DOCUMENT_RECIPE.into(),
        )
        .unwrap()
    }
    pub fn limits(&self) -> super::model_api::RpcLimits {
        let mut limits = self.client.limits();
        if self.expected.embedding_is_scalar() {
            limits.max_texts = 1;
        }
        limits
    }
    pub async fn infer<S, F>(
        &self,
        operation: super::model_api::EmbedOperation,
        connect: F,
    ) -> Result<Vec<Vec<f32>>, super::model_api::ModelServiceError>
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
