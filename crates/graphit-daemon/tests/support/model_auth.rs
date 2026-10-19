//! Physical public Auth producers; no fabricated lease or child authority.
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, RoleIssuer, ScopeCapture,
    ServiceAdmissionLimits, ServiceAuthorityInput, ServiceOrigin, ServicePurpose, ServiceRuleInput,
    SourceAuthorityPolicy, TtlPolicy,
    profile::{
        AiService, LocalCredentials, LocalDevice, LocalExecution, ProfileCredentials, ProfileData,
        ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
        Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeBindingRecord, ScopeLimits},
    auth_document::{AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect},
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

use crate::model_service_revocation as service_revocation;
pub use service_revocation::revoke_service;

pub fn request(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
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
pub fn cpu() -> LocalExecution {
    LocalExecution {
        device: LocalDevice::Cpu,
        device_id: 0,
    }
}
pub fn profile(username: &str) -> ProfileData {
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
pub struct AuthFixture {
    pub runtime: Arc<RuntimeKeyOwner>,
    pub store: Arc<AuthStore>,
    installation: CapturedPublicationScope,
    record: ScopeBindingRecord,
    _directory: tempfile::TempDir,
}
impl AuthFixture {
    #[allow(dead_code, reason = "used by the dedicated child integration target")]
    pub fn installation_root(&self) -> &std::path::Path {
        self.installation.owner_root()
    }
    pub fn new(
        ctx: &RequestContext<'_>,
        embedding: AiService,
        rerank: AiService,
        activate: bool,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let root = directory.path().join("project");
        std::fs::create_dir(&home).unwrap();
        std::fs::create_dir(&root).unwrap();
        let paths = InstallationPaths::resolve(Brand::default(), &root, Some(&home), None).unwrap();
        let private = installation_bootstrap::prepare(
            &paths,
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
        let id = InstallationId::new("fixture-installation").unwrap();
        let scopes = ScopeLimits::new(16, 4096, 128).unwrap();
        let installation = CapturedPublicationScope::installation(
            &paths,
            id.clone(),
            [root],
            scopes,
            ctx.snapshot(),
        )
        .unwrap();
        let record = installation.binding().unwrap();
        let store_scope = CapturedPublicationScope::installation(
            &paths,
            id,
            [installation.owner_root().to_owned()],
            scopes,
            ctx.snapshot(),
        )
        .unwrap();
        let physical = AuthDocumentStore::capture(
            store_scope,
            AuthDocumentLimits::new(4096, 65536, 256, 65536).unwrap(),
            ctx.snapshot(),
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
                        embedding,
                        rerank,
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
                if activate {
                    doc.activate("fixture", doc.revision(), ctx)?;
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(result.effect(), AuthPublicationEffect::Applied);
        assert!(result.failure().is_none());
        assert!(result.cleanup_failure().is_none());
        Self {
            runtime,
            store,
            installation,
            record,
            _directory: directory,
        }
    }
    pub fn issuer(
        &self,
        ctx: &RequestContext<'_>,
        purpose: ServicePurpose,
        execution: LocalExecution,
    ) -> RoleIssuer {
        let rules = [ServiceRuleInput::Local {
            origin: ServiceOrigin::Runtime,
            provider: "service",
            purpose,
            execution,
        }];
        let ordinary: [Permission; 0] = [];
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
            self.runtime.clone(),
            self.store.clone(),
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
    pub fn revoke_service(&self, ctx: &RequestContext<'_>) {
        revoke_service(&self.store, ctx);
    }
}
