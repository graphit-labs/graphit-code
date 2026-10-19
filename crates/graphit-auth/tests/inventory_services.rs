//! Real Core/AuthStore fixtures; no HTTP or installed-host qualification is inferred.
use graphit_auth::{
    AdminSessionPolicy, AuthError, CredentialSource, ExternalEffect, IssuerAuthorityPolicy,
    IssuerLimits, Permission, RemoteServiceBindingInput, RoleIssuer, ScopeCapture,
    ServiceAdmissionLimits, ServiceAuthorityInput, ServiceBindingView, ServiceMode, ServiceOrigin,
    ServicePurpose, ServiceRuleInput, ServiceUseError, SourceAuthorityPolicy, TtlPolicy,
    profile::{
        AiService, BrokerService, DirectProtocol, LocalCredentials, ProfileCredentials,
        ProfileData, ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location,
        S3Source, Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeBindingRecord, ScopeLimits},
    auth_document::{AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationIdentityStore,
    project_paths::InstallationPaths,
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use secrecy::SecretString;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const MCP: &str = "inventory-fixture-mcp-key";
const KEY: &str = "inventory-fixture-broker-key";
const ENDPOINT: &str = "https://broker.example/api";
const REMOTE: Permission = Permission::External(ExternalEffect::RemoteRequest);

fn context(cancel: &Cancellation) -> RequestContext<'_> {
    context_until(cancel, Instant::now() + Duration::from_secs(120))
}
fn context_until(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 64,
            max_nodes: 65536,
            max_scalar_bytes: 65536,
            max_collection_entries: 128,
        },
        cancel,
        deadline,
    )
    .unwrap()
}
fn limits() -> IssuerLimits {
    IssuerLimits {
        max_permissions: 64,
        max_scope_bytes: 65536,
        max_parent_bearer_bytes: 1024,
    }
}
fn rule(endpoint: &str, origin: ServiceOrigin, purpose: ServicePurpose) -> ServiceRuleInput<'_> {
    ServiceRuleInput::Remote {
        origin,
        binding: RemoteServiceBindingInput::BrokerInventory {
            provider: "inventory",
            purpose,
            endpoint,
        },
        credential: CredentialSource::LocalBrokerKey,
        incoming: None,
    }
}
fn policy(
    rules: &[ServiceRuleInput<'_>],
    ordinary: &[Permission],
    bounds: ServiceAdmissionLimits,
    cap: IssuerLimits,
) -> Result<IssuerAuthorityPolicy, AuthError> {
    let source = || SourceAuthorityPolicy {
        ordinary,
        administrative: &[Permission::ManageAccessGrants, REMOTE],
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    IssuerAuthorityPolicy::new(
        source(),
        source(),
        cap,
        ServiceAuthorityInput {
            rules,
            limits: bounds,
            native_refresh_margin: Duration::ZERO,
        },
    )
}
fn bounds() -> ServiceAdmissionLimits {
    ServiceAdmissionLimits {
        max_rules: 16,
        max_binding_bytes: 8192,
    }
}
fn config(endpoint: Option<&str>, service: AiService) -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::Local {
            allow_aws_credential_chain: false,
            broker: endpoint.map(|endpoint| BrokerService {
                endpoint: endpoint.into(),
                mcp_resource: None,
                allow_anonymous: true,
            }),
        },
        embedding: service.clone(),
        rerank: service,
        s3: S3Location {
            bucket: None,
            region: None,
            endpoint: None,
            prefix: None,
            source: S3Source::LocalCredentials,
        },
    }
}
fn secret(raw: &str) -> Secret {
    Secret::new(SecretString::from(raw))
}
fn data(key: Option<&str>, direct: bool) -> ProfileData {
    ProfileData {
        name: "local-inventory".into(),
        provider: "inventory".into(),
        username: "fixture".into(),
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Local(LocalCredentials {
            anonymous: false,
            mcp_key: Some(secret(MCP)),
            broker_key: key.map(secret),
            embedding_api_key: direct.then(|| secret("embedding-fixture")),
            rerank_api_key: direct.then(|| secret("rerank-fixture")),
            s3: None,
        }),
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    realm: CapturedPublicationScope,
    binding: ScopeBindingRecord,
    runtime: Arc<RuntimeKeyOwner>,
    store: Arc<AuthStore>,
    paths: InstallationPaths,
}
impl Fixture {
    fn new(ctx: &RequestContext<'_>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir(&root).unwrap();
        let owner = dir.path().join("installation");
        let paths = InstallationPaths::resolve(
            Brand::new("inventory-sdk-test").unwrap(),
            &root,
            None,
            Some(&owner),
        )
        .unwrap();
        let private = installation_bootstrap::prepare(
            &paths,
            BootstrapLimits::new(65536, 256).unwrap(),
            ctx.snapshot(),
        )
        .into_installation()
        .unwrap();
        let identity = InstallationIdentityStore::new(&paths)
            .unwrap()
            .ensure(Duration::from_secs(5), ctx.snapshot().cancellation())
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
        let capture = || {
            CapturedPublicationScope::installation(
                &paths,
                identity.unit_id().clone(),
                [paths.root().to_owned(), root.clone()],
                ScopeLimits::new(16, 4096, 128).unwrap(),
                ctx.snapshot(),
            )
            .unwrap()
        };
        let realm = capture();
        let binding = realm.binding().unwrap();
        let store = Arc::new(
            AuthStore::new(
                AuthDocumentStore::capture(
                    capture(),
                    AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
                    ctx.snapshot(),
                )
                .unwrap(),
                ProfileLimits {
                    bytes: 65536,
                    nodes: 65536,
                    scalar_bytes: 65536,
                    collection_entries: 128,
                    providers: 16,
                    profiles: 16,
                },
                ctx,
            )
            .unwrap(),
        );
        Self {
            _dir: dir,
            realm,
            binding,
            runtime,
            store,
            paths,
        }
    }
    fn put(&self, cfg: ProviderConfig, data: ProfileData, ctx: &RequestContext<'_>) {
        let before = self.store.load(ctx).unwrap();
        let result = self
            .store
            .mutate(before, ctx, |doc, ctx| {
                doc.add_provider("inventory".into(), cfg, doc.revision(), 1, ctx)?;
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("inventory").unwrap().revision(),
                    profile_revision: None,
                };
                let name = data.name.clone();
                doc.replace_profile_data(data, expected, 1, ctx)?;
                doc.activate(&name, doc.revision(), ctx)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    }
    fn issuer(
        &self,
        rules: &[ServiceRuleInput<'_>],
        ordinary: &[Permission],
        ctx: &RequestContext<'_>,
    ) -> RoleIssuer {
        RoleIssuer::new(
            ScopeCapture {
                live: &self.realm,
                record: &self.binding,
            },
            Arc::clone(&self.runtime),
            Arc::clone(&self.store),
            policy(rules, ordinary, bounds(), limits()).unwrap(),
            TtlPolicy {
                ttl: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
                refresh_margin: Duration::from_secs(120),
            },
            ctx,
        )
        .unwrap()
    }
    fn key(&self) -> String {
        std::fs::read_to_string(self.paths.root().join("daemon/mcp.key")).unwrap()
    }
}
fn denied<T>(value: Result<T, ServiceUseError>) {
    assert!(matches!(
        value,
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
}

#[test]
fn runtime_and_local_inventory_use_real_broker_key_with_ai_disabled() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    f.put(
        config(
            Some("https://BROKER.example:443/a/../api"),
            AiService::Disabled,
        ),
        data(Some(KEY), false),
        &ctx,
    );
    let rules = [
        rule(
            ENDPOINT,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryRead,
        ),
        rule(
            ENDPOINT,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryPublish,
        ),
        rule(
            ENDPOINT,
            ServiceOrigin::Local,
            ServicePurpose::InventoryRead,
        ),
        rule(
            ENDPOINT,
            ServiceOrigin::Local,
            ServicePurpose::InventoryPublish,
        ),
    ];
    let issuer = f.issuer(&rules, &[REMOTE], &ctx);
    let key = f.key();
    let runtime = issuer.authenticate_runtime(&key, &ctx).unwrap();
    let local = issuer.authenticate_local(MCP, &ctx).unwrap();
    let revision = f.store.load(&ctx).unwrap().document().revision();
    let expected_provider_revision = f
        .store
        .load(&ctx)
        .unwrap()
        .document()
        .provider("inventory")
        .unwrap()
        .revision();
    for caller in [&runtime, &local] {
        for purpose in [
            ServicePurpose::InventoryRead,
            ServicePurpose::InventoryPublish,
        ] {
            let mut selection = issuer.capture_service(caller, purpose).unwrap();
            assert_eq!(selection.mode().unwrap(), ServiceMode::BrokerKey);
            assert!(Arc::ptr_eq(
                selection.original().unwrap().snapshot(),
                ctx.snapshot()
            ));
            assert!(matches!(selection.local(), Err(ServiceUseError::WrongMode)));
            let lease = selection.credential().unwrap();
            assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
            assert_eq!(lease.credential().unwrap(), Some(KEY));
            assert!(lease.user_subject().unwrap().is_none());
            match lease.binding().unwrap() {
                ServiceBindingView::BrokerInventory {
                    provider,
                    purpose: actual,
                    endpoint,
                    provider_revision,
                } => {
                    assert_eq!(provider, "inventory");
                    assert_eq!(actual, purpose);
                    assert_eq!(endpoint, ENDPOINT);
                    assert_eq!(provider_revision, expected_provider_revision);
                }
                _ => panic!("wrong binding"),
            }
            lease.check().unwrap();
        }
    }
    assert_eq!(f.store.load(&ctx).unwrap().document().revision(), revision);
}

#[test]
fn typed_inventory_bindings_reject_ai_purposes_other_modes_and_origins() {
    for purpose in [ServicePurpose::Embedding, ServicePurpose::Rerank] {
        assert!(matches!(
            policy(
                &[rule(ENDPOINT, ServiceOrigin::Runtime, purpose)],
                &[REMOTE],
                bounds(),
                limits()
            ),
            Err(AuthError::Invalid)
        ));
    }
    for credential in [
        CredentialSource::DirectKey,
        CredentialSource::Anonymous,
        CredentialSource::IncomingBearer,
    ] {
        let ServiceRuleInput::Remote {
            origin,
            binding,
            incoming,
            ..
        } = rule(
            ENDPOINT,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryRead,
        )
        else {
            unreachable!()
        };
        assert!(matches!(
            policy(
                &[ServiceRuleInput::Remote {
                    origin,
                    binding,
                    credential,
                    incoming
                }],
                &[REMOTE],
                bounds(),
                limits()
            ),
            Err(AuthError::Invalid)
        ));
    }
    assert!(matches!(
        policy(
            &[rule(
                ENDPOINT,
                ServiceOrigin::BrokerIncoming,
                ServicePurpose::InventoryRead
            )],
            &[REMOTE],
            bounds(),
            limits()
        ),
        Err(AuthError::Invalid)
    ));
    for purpose in [
        ServicePurpose::InventoryRead,
        ServicePurpose::InventoryPublish,
    ] {
        let old = ServiceRuleInput::Remote {
            origin: ServiceOrigin::Runtime,
            binding: RemoteServiceBindingInput::Broker {
                provider: "inventory",
                purpose,
                endpoint: ENDPOINT,
                resource: None,
            },
            credential: CredentialSource::LocalBrokerKey,
            incoming: None,
        };
        assert!(matches!(
            policy(&[old], &[REMOTE], bounds(), limits()),
            Err(AuthError::Invalid)
        ));
        let direct = ServiceRuleInput::Remote {
            origin: ServiceOrigin::Runtime,
            binding: RemoteServiceBindingInput::Direct {
                provider: "inventory",
                purpose,
                protocol: DirectProtocol::Cohere,
                endpoint: ENDPOINT,
                model: "rerank",
                model_revision: "fixture-deployment-2026-10",
                recipe: "fixture-vector-v1",
                dimensions: 0,
                request_dimensions: None,
                batch_endpoint: None,
            },
            credential: CredentialSource::DirectKey,
            incoming: None,
        };
        assert!(matches!(
            policy(&[direct], &[REMOTE], bounds(), limits()),
            Err(AuthError::Invalid)
        ));
        let local = ServiceRuleInput::Local {
            origin: ServiceOrigin::Runtime,
            provider: "inventory",
            purpose,
            execution: graphit_auth::profile::LocalExecution {
                device: graphit_auth::profile::LocalDevice::Cpu,
                device_id: 0,
            },
        };
        assert!(matches!(
            policy(&[local], &[REMOTE], bounds(), limits()),
            Err(AuthError::Invalid)
        ));
    }
}

#[test]
fn inventory_url_and_raw_canonical_budgets_are_bounded() {
    let pair_raw = "https://BROKER.example:443/a/../api";
    let pair = [
        rule(
            pair_raw,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryRead,
        ),
        rule(
            pair_raw,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryPublish,
        ),
    ];
    assert!(matches!(
        policy(
            &pair,
            &[REMOTE],
            ServiceAdmissionLimits {
                max_rules: 2,
                max_binding_bytes: 2 * ("inventory".len() + pair_raw.len()) - 1,
            },
            limits()
        ),
        Err(AuthError::Limit)
    ));
    for endpoint in [
        "https://broker.example/api?x=1",
        "https://broker.example/api#x",
        "https://u@broker.example/api",
        "https://@broker.example/api",
        "https://:@broker.example/api",
        "https://broker.example:443/api ",
        "file:///api",
        "",
        "https://broker.example/\napi",
    ] {
        assert!(matches!(
            policy(
                &[rule(
                    endpoint,
                    ServiceOrigin::Runtime,
                    ServicePurpose::InventoryRead
                )],
                &[REMOTE],
                bounds(),
                limits()
            ),
            Err(AuthError::Invalid)
        ));
    }
    let input = rule(
        "https://b",
        ServiceOrigin::Runtime,
        ServicePurpose::InventoryRead,
    );
    assert!(matches!(
        policy(
            &[input],
            &[REMOTE],
            ServiceAdmissionLimits {
                max_rules: 1,
                max_binding_bytes: "inventory".len() + "https://b".len()
            },
            limits()
        ),
        Err(AuthError::Limit)
    ));
    assert!(
        policy(
            &[input],
            &[REMOTE],
            ServiceAdmissionLimits {
                max_rules: 1,
                max_binding_bytes: "inventory".len() + "https://b/".len()
            },
            limits()
        )
        .is_ok()
    );
    let raw = "https://BROKER.example:443/a/../api";
    assert!(matches!(
        policy(
            &[rule(
                raw,
                ServiceOrigin::Runtime,
                ServicePurpose::InventoryRead
            )],
            &[REMOTE],
            ServiceAdmissionLimits {
                max_rules: 1,
                max_binding_bytes: "inventory".len() + ENDPOINT.len()
            },
            limits()
        ),
        Err(AuthError::Limit)
    ));
    assert!(matches!(
        policy(&[input, input], &[REMOTE], bounds(), limits()),
        Err(AuthError::Invalid)
    ));
    assert!(matches!(
        policy(
            &[input],
            &[REMOTE],
            ServiceAdmissionLimits {
                max_rules: 0,
                max_binding_bytes: 8192
            },
            limits()
        ),
        Err(AuthError::Invalid)
    ));
    let mut cap = limits();
    cap.max_scope_bytes = "https://b".len();
    assert!(matches!(
        policy(&[input], &[REMOTE], bounds(), cap),
        Err(AuthError::Limit)
    ));
}

#[test]
fn inventory_requires_exact_endpoint_purpose_and_real_remote_ceiling() {
    for (configured, admitted, allowed) in [
        (ENDPOINT, ENDPOINT, false),
        (ENDPOINT, "https://broker.example/api/", true),
        (ENDPOINT, "https://broker.example/other", true),
        ("https://broker.example/api?x=1", ENDPOINT, true),
        ("https://@broker.example/api", ENDPOINT, true),
        ("https://:@broker.example/api", ENDPOINT, true),
    ] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        f.put(
            config(Some(configured), AiService::Disabled),
            data(Some(KEY), false),
            &ctx,
        );
        let rules = [rule(
            admitted,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryRead,
        )];
        let issuer = f.issuer(&rules, if allowed { &[REMOTE] } else { &[] }, &ctx);
        let key = f.key();
        let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
        if configured.contains('?') || configured.contains('@') {
            assert!(matches!(
                issuer.capture_service(&caller, ServicePurpose::InventoryRead),
                Err(ServiceUseError::Auth(AuthError::Invalid))
            ));
        } else {
            denied(issuer.capture_service(&caller, ServicePurpose::InventoryRead));
        }
        denied(issuer.capture_service(&caller, ServicePurpose::InventoryPublish));
    }
}

#[test]
fn missing_broker_key_is_typed_without_anonymous_fallback() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    f.put(
        config(Some(ENDPOINT), AiService::Disabled),
        data(None, false),
        &ctx,
    );
    let rules = [rule(
        ENDPOINT,
        ServiceOrigin::Runtime,
        ServicePurpose::InventoryRead,
    )];
    let issuer = f.issuer(&rules, &[REMOTE], &ctx);
    let key = f.key();
    let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    assert!(matches!(
        selection.credential(),
        Err(ServiceUseError::MissingCredential)
    ));
}

#[test]
fn inventory_borrow_rechecks_revocation_and_original_cancellation() {
    for revoke in [false, true] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        f.put(
            config(Some(ENDPOINT), AiService::Disabled),
            data(Some(KEY), false),
            &ctx,
        );
        let rules = [rule(
            ENDPOINT,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryPublish,
        )];
        let issuer = f.issuer(&rules, &[REMOTE], &ctx);
        let key = f.key();
        let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::InventoryPublish)
            .unwrap();
        let mut checked_selection = issuer
            .capture_service(&caller, ServicePurpose::InventoryPublish)
            .unwrap();
        checked_selection.original().unwrap();
        let lease = selection.credential().unwrap();
        assert_eq!(lease.credential().unwrap(), Some(KEY));
        if revoke {
            let session = issuer.bootstrap_admin(&caller).unwrap();
            let parent = issuer.admin_parent(&caller, &session).unwrap();
            issuer.revoke_service_policy(&parent).unwrap();
            denied(checked_selection.original());
            denied(lease.check());
            denied(lease.credential());
            denied(lease.binding());
        } else {
            cancel.cancel();
            assert!(matches!(
                checked_selection.original(),
                Err(ServiceUseError::Profile(
                    graphit_auth::profile::ProfileError::Cancelled
                ))
            ));
            assert!(matches!(
                lease.check(),
                Err(ServiceUseError::Profile(
                    graphit_auth::profile::ProfileError::Cancelled
                ))
            ));
            assert!(lease.credential().is_err());
            assert!(lease.original().is_err());
        }
    }
}

#[test]
fn broker_local_accepts_direct_ai_without_repurposing_its_keys() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let mut cfg = config(Some(ENDPOINT), AiService::Disabled);
    cfg.embedding = AiService::Direct {
        protocol: DirectProtocol::OpenAi,
        endpoint: "https://ai.example".into(),
        model: "embedding".into(),
        model_revision: "fixture-deployment-2026-10".into(),
        recipe: "fixture-vector-v1".into(),
        dimensions: 3,
        request_dimensions: None,
        batch_endpoint: None,
    };
    cfg.rerank = AiService::Direct {
        protocol: DirectProtocol::Cohere,
        endpoint: "https://ai.example".into(),
        model: "rerank".into(),
        model_revision: "fixture-deployment-2026-10".into(),
        recipe: "fixture-vector-v1".into(),
        dimensions: 0,
        request_dimensions: None,
        batch_endpoint: None,
    };
    f.put(cfg, data(Some(KEY), true), &ctx);
    let rules = [rule(
        ENDPOINT,
        ServiceOrigin::Runtime,
        ServicePurpose::InventoryRead,
    )];
    let issuer = f.issuer(&rules, &[REMOTE], &ctx);
    let key = f.key();
    let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    assert_eq!(
        selection.credential().unwrap().credential().unwrap(),
        Some(KEY)
    );
}

#[test]
fn inventory_lease_rejects_changed_physical_profile() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    f.put(
        config(Some(ENDPOINT), AiService::Disabled),
        data(Some(KEY), false),
        &ctx,
    );
    let rules = [rule(
        ENDPOINT,
        ServiceOrigin::Runtime,
        ServicePurpose::InventoryRead,
    )];
    let issuer = f.issuer(&rules, &[REMOTE], &ctx);
    let key = f.key();
    let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    let mut checked_selection = issuer
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    checked_selection.original().unwrap();
    let lease = selection.credential().unwrap();
    let publication = f
        .store
        .mutate(f.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("inventory").unwrap().revision(),
                profile_revision: Some(doc.profile("local-inventory").unwrap().revision()),
            };
            doc.replace_profile_data(data(Some("changed-fixture-key"), false), expected, 2, ctx)
        })
        .unwrap();
    assert_eq!(publication.effect(), AuthPublicationEffect::Applied);
    assert!(checked_selection.original().is_err());
    assert!(lease.check().is_err());
    assert!(lease.credential().is_err());
    assert!(lease.binding().is_err());
}

#[test]
fn inventory_requires_broker_configuration_and_same_issuer_caller() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    f.put(config(None, AiService::Disabled), data(None, false), &ctx);
    let rules = [rule(
        ENDPOINT,
        ServiceOrigin::Runtime,
        ServicePurpose::InventoryRead,
    )];
    let issuer = f.issuer(&rules, &[REMOTE], &ctx);
    let other = f.issuer(&rules, &[REMOTE], &ctx);
    let key = f.key();
    let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
    denied(issuer.capture_service(&caller, ServicePurpose::InventoryRead));
    denied(other.capture_service(&caller, ServicePurpose::InventoryRead));
}

#[test]
fn inventory_lease_retains_original_deadline() {
    let cancel = Cancellation::default();
    let setup = context(&cancel);
    let f = Fixture::new(&setup);
    f.put(
        config(Some(ENDPOINT), AiService::Disabled),
        data(Some(KEY), false),
        &setup,
    );
    let rules = [rule(
        ENDPOINT,
        ServiceOrigin::Runtime,
        ServicePurpose::InventoryRead,
    )];
    let issuer = f.issuer(&rules, &[REMOTE], &setup);
    let deadline = Instant::now() + Duration::from_secs(1);
    let ctx = RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 64,
            max_nodes: 65536,
            max_scalar_bytes: 65536,
            max_collection_entries: 128,
        },
        &cancel,
        deadline,
    )
    .unwrap();
    let key = f.key();
    let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    let mut checked_selection = issuer
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    checked_selection.original().unwrap();
    let lease = selection.credential().unwrap();
    assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
    std::thread::sleep(
        deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
    );
    assert!(matches!(
        lease.check(),
        Err(ServiceUseError::Profile(
            graphit_auth::profile::ProfileError::Deadline
        ))
    ));
    assert!(matches!(
        checked_selection.original(),
        Err(ServiceUseError::Profile(
            graphit_auth::profile::ProfileError::Deadline
        ))
    ));
    assert!(lease.original().is_err());
    assert!(lease.credential().is_err());
}

#[test]
fn native_inventory_original_precedes_token_verification() {
    use graphit_auth::profile::OidcTokens;

    let cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(120);
    let ctx = context_until(&cancel, deadline);
    let other = context_until(&cancel, deadline);
    let f = Fixture::new(&ctx);
    let mut cfg = config(Some(ENDPOINT), AiService::Disabled);
    cfg.kind = ProviderKind::Broker(BrokerService {
        endpoint: ENDPOINT.into(),
        mcp_resource: None,
        allow_anonymous: false,
    });
    cfg.s3.source = S3Source::BrokerScope;
    f.put(
        cfg,
        ProfileData {
            name: "native".into(),
            provider: "inventory".into(),
            username: "fixture".into(),
            organization: None,
            teams: vec![],
            credentials: ProfileCredentials::Broker {
                issuer: ENDPOINT.into(),
                subject: "fixture-native".into(),
                s3_disabled: true,
                tokens: OidcTokens {
                    access_token: secret("not-a-jwt"),
                    refresh_token: None,
                    id_token: secret("not-an-id-token"),
                    expires_at: 2000000000,
                },
            },
        },
        &ctx,
    );
    let rules = [ServiceRuleInput::Remote {
        origin: ServiceOrigin::Runtime,
        binding: RemoteServiceBindingInput::BrokerInventory {
            provider: "inventory",
            purpose: ServicePurpose::InventoryRead,
            endpoint: ENDPOINT,
        },
        credential: CredentialSource::NativeAccess,
        incoming: None,
    }];
    let issuer = f.issuer(&rules, &[REMOTE], &ctx);
    let key = f.key();
    let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    assert_eq!(selection.mode().unwrap(), ServiceMode::BrokerNative);
    assert!(Arc::ptr_eq(
        selection.original().unwrap().snapshot(),
        ctx.snapshot()
    ));
    assert!(!Arc::ptr_eq(
        selection.original().unwrap().snapshot(),
        other.snapshot()
    ));
    assert!(matches!(
        selection.credential(),
        Err(ServiceUseError::WrongMode)
    ));
}

#[test]
fn native_broker_config_is_independent_of_ai_but_inventory_rejects_native_provider() {
    use graphit_auth::profile::OidcTokens;
    for direct in [false, true] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let mut cfg = config(Some(ENDPOINT), AiService::Disabled);
        cfg.kind = ProviderKind::Broker(BrokerService {
            endpoint: ENDPOINT.into(),
            mcp_resource: None,
            allow_anonymous: false,
        });
        cfg.s3.source = S3Source::BrokerScope;
        if direct {
            cfg.embedding = AiService::Direct {
                protocol: DirectProtocol::OpenAi,
                endpoint: "https://ai.example".into(),
                model: "embedding".into(),
                model_revision: "fixture-deployment-2026-10".into(),
                recipe: "fixture-vector-v1".into(),
                dimensions: 3,
                request_dimensions: None,
                batch_endpoint: None,
            };
            cfg.rerank = AiService::Direct {
                protocol: DirectProtocol::Cohere,
                endpoint: "https://ai.example".into(),
                model: "rerank".into(),
                model_revision: "fixture-deployment-2026-10".into(),
                recipe: "fixture-vector-v1".into(),
                dimensions: 0,
                request_dimensions: None,
                batch_endpoint: None,
            };
        }
        // Provider configuration validation is independent of whether a native
        // profile can supply a particular Direct AI credential.
        let result = f
            .store
            .mutate(f.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                doc.add_provider("inventory".into(), cfg, doc.revision(), 1, ctx)
            })
            .unwrap();
        assert_eq!(result.effect(), AuthPublicationEffect::Applied);
        if direct {
            continue;
        }
        let result = f
            .store
            .mutate(f.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("inventory").unwrap().revision(),
                    profile_revision: None,
                };
                doc.replace_profile_data(
                    ProfileData {
                        name: "native".into(),
                        provider: "inventory".into(),
                        username: "fixture".into(),
                        organization: None,
                        teams: vec![],
                        credentials: ProfileCredentials::Broker {
                            issuer: ENDPOINT.into(),
                            subject: "fixture-native".into(),
                            s3_disabled: true,
                            tokens: OidcTokens {
                                access_token: secret("opaque-structural-token"),
                                refresh_token: None,
                                id_token: secret("opaque-structural-id-token"),
                                expires_at: 2000000000,
                            },
                        },
                    },
                    expected,
                    2,
                    ctx,
                )?;
                doc.activate("native", doc.revision(), ctx)
            })
            .unwrap();
        assert_eq!(result.effect(), AuthPublicationEffect::Applied);
        let rules = [rule(
            ENDPOINT,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryRead,
        )];
        let issuer = f.issuer(&rules, &[REMOTE], &ctx);
        let key = f.key();
        let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
        denied(issuer.capture_service(&caller, ServicePurpose::InventoryRead));
    }
}

#[test]
fn ai_broker_without_configured_broker_remains_invalid() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let result = f
        .store
        .mutate(f.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            doc.add_provider(
                "inventory".into(),
                config(None, AiService::Broker),
                doc.revision(),
                1,
                ctx,
            )
        });
    assert!(matches!(
        result,
        Err(graphit_auth::store::AuthStoreError::Profile(
            graphit_auth::profile::ProfileError::InvalidDocument
        ))
    ));
}

#[test]
fn verified_runtime_role_does_not_admit_inventory() {
    use graphit_auth::{ProjectCapture, RoleDefinition, ScopeInput};
    use graphit_core::{
        native_path::NativePath,
        project_identity::{ProjectIdentityStore, ProjectMetadata},
        project_paths::ProjectPaths,
        version_context::{Checkout, VersionContext},
    };
    use graphit_hub::{
        ArtifactType, NativeBinding, ProjectionLimits, SourceOrigin, SourceRecord, SourceRef,
        native::Role,
    };
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    f.put(
        config(Some(ENDPOINT), AiService::Disabled),
        data(Some(KEY), false),
        &ctx,
    );
    let rules = [
        rule(
            ENDPOINT,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryRead,
        ),
        rule(
            ENDPOINT,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryPublish,
        ),
    ];
    let issuer = f.issuer(&rules, &[REMOTE], &ctx);
    let key = f.key();
    let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let root = f._dir.path().join("project");
    let paths = ProjectPaths::at(&root, &f.paths).unwrap();
    let identity = ProjectIdentityStore::at(&paths)
        .writer(Duration::from_secs(5), ctx.snapshot().cancellation())
        .unwrap()
        .ensure(&ProjectMetadata::new("inventory-role-fixture").unwrap())
        .unwrap();
    let source = SourceRef::restore(
        SourceRecord {
            remote_file_origin: None,
            archive_sha256: None,
            origin: SourceOrigin::LocalLink,
            producer: identity.id(),
            artifact_type: ArtifactType::Agent,
            id: "roles/coordinator".into(),
            physical_id: None,
            version: "1.0.0".into(),
            root: NativePath::new(root.clone()).unwrap(),
        },
        ProjectionLimits::new(128, 4096, 32, 128, 32).unwrap(),
    )
    .unwrap();
    let definition = RoleDefinition::new(
        Role::Coordinator,
        &source,
        NativeBinding::KiroV3,
        &[REMOTE],
        [1; 32],
        limits(),
    )
    .unwrap();
    let reference = definition.reference().clone();
    issuer.register_definition(&parent, definition).unwrap();
    let project = CapturedPublicationScope::project(
        &paths,
        identity.id(),
        ScopeLimits::new(16, 4096, 128).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    let project_record = project.binding().unwrap();
    let version = VersionContext::select(&paths, Checkout::NonGit, "main", None, "").unwrap();
    let scope = || ScopeInput {
        installation: ScopeCapture {
            live: &f.realm,
            record: &f.binding,
        },
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &project,
                record: &project_record,
            },
            version: &version,
        }),
        providers: &[],
        binding: NativeBinding::KiroV3,
        global_version: "",
    };
    let run = issuer
        .start_run(&parent, &scope(), Duration::from_secs(600))
        .unwrap();
    let bearer = issuer
        .issue(&parent, &run, &reference, &[REMOTE], &scope())
        .unwrap();
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, scope(), &ctx)
        .unwrap();
    assert_eq!(
        grant.authorize(REMOTE).unwrap(),
        graphit_auth::Obligation::Unconditional
    );
    for purpose in [
        ServicePurpose::InventoryRead,
        ServicePurpose::InventoryPublish,
    ] {
        denied(issuer.capture_role_service(&grant, purpose));
    }
}

#[tokio::test]
async fn verified_incoming_bearer_is_denied_without_inventory_io() {
    use aws_lc_rs::{
        rand::SystemRandom,
        signature::{Ed25519KeyPair, KeyPair},
    };
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use graphit_auth::broker::{
        BrokerPolicy, BrokerRequestContext, BrokerVerifier, JsonLimits, VerifierLimits,
    };
    use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
    use serde_json::json;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    struct ListenerTask(tokio::task::JoinHandle<()>);
    impl Drop for ListenerTask {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    const RESOURCE: &str = "https://mcp.example/graphit";
    let signing_key = Ed25519KeyPair::from_pkcs8(
        Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .unwrap()
            .as_ref(),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let metadata = json!({"version":"1","issuer":origin,"authentication":{"type":"oidc","issuer":origin,
        "client_id":"graphit-cli","scopes":["openid","profile"],"redirect_uri_path":"/callback",
        "audiences":["broker-api"],"access_token_audience":"broker-api","mcp_resources":[RESOURCE]}}).to_string();
    let standard = json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),
        "token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/keys"),
        "grant_types_supported":["authorization_code","refresh_token"],
        "code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],
        "id_token_signing_alg_values_supported":["EdDSA"]}).to_string();
    let jwks = json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(signing_key.public_key().as_ref()),
        "kid":"inventory-actual","alg":"EdDSA","use":"sig","key_ops":["verify"]}]}).to_string();
    let requests = Arc::new(AtomicUsize::new(0));
    let counts = Arc::clone(&requests);
    let _listener = ListenerTask(tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut input = Vec::new();
            let mut buffer = [0u8; 1024];
            while !input.windows(4).any(|b| b == b"\r\n\r\n") {
                let Ok(read) = socket.read(&mut buffer).await else {
                    return;
                };
                if read == 0 || input.len() + read > 16384 {
                    return;
                }
                input.extend_from_slice(&buffer[..read]);
            }
            let request = std::str::from_utf8(&input).unwrap();
            let path = request
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap();
            counts.fetch_add(1, Ordering::SeqCst);
            let body = match path {
                "/.well-known/graphit-broker" => metadata.as_str(),
                "/.well-known/openid-configuration" => standard.as_str(),
                "/keys" => jwks.as_str(),
                _ => panic!("unexpected verifier request"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    }));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD
            .encode(json!({"alg":"EdDSA","kid":"inventory-actual","typ":"JWT"}).to_string()),
        URL_SAFE_NO_PAD.encode(
            json!({"iss":origin,"sub":"inventory-incoming","aud":[RESOURCE],
            "client_id":"another-registered-client","jti":"actual-inventory-incoming",
            "token_use":"access","iat":now.saturating_sub(1),"exp":now + 3600})
            .to_string()
        )
    );
    let raw = format!(
        "{message}.{}",
        URL_SAFE_NO_PAD.encode(signing_key.sign(message.as_bytes()).as_ref())
    );
    let verifier = BrokerVerifier::new(
        BrokerPolicy {
            endpoint: &origin,
            mcp_resource: Some(RESOURCE),
            provider_revision: 1,
            network: ClientPolicy {
                origin: &origin,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[],
            },
        },
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
        },
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
    )
    .unwrap();
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let caller = verifier
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    assert_eq!(caller.username(), None);
    let rules = [
        rule(
            ENDPOINT,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryRead,
        ),
        rule(
            ENDPOINT,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryPublish,
        ),
    ];
    let issuer = f.issuer(&rules, &[REMOTE], &ctx);
    let before = requests.load(Ordering::SeqCst);
    assert!(before > 0);
    for purpose in [
        ServicePurpose::InventoryRead,
        ServicePurpose::InventoryPublish,
    ] {
        denied(issuer.capture_incoming_service(&caller, &verifier, &ctx, purpose));
    }
    assert_eq!(requests.load(Ordering::SeqCst), before);
    assert!(!f.paths.root().join("auth.json").exists());
}

#[test]
fn actual_provider_revision_invalidates_borrow_and_new_capture_is_stale() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    f.put(
        config(Some(ENDPOINT), AiService::Disabled),
        data(Some(KEY), false),
        &ctx,
    );
    let rules = [rule(
        ENDPOINT,
        ServiceOrigin::Runtime,
        ServicePurpose::InventoryRead,
    )];
    let issuer = f.issuer(&rules, &[REMOTE], &ctx);
    let key = f.key();
    let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    let mut checked_selection = issuer
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    checked_selection.original().unwrap();
    let lease = selection.credential().unwrap();
    let publication = f
        .store
        .mutate(f.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            let revision = doc.provider("inventory").unwrap().revision();
            doc.update_provider(
                "inventory",
                config(Some(ENDPOINT), AiService::Disabled),
                doc.revision(),
                revision,
                2,
                ctx,
            )
        })
        .unwrap();
    assert_eq!(publication.effect(), AuthPublicationEffect::Applied);
    assert!(checked_selection.original().is_err());
    assert!(lease.check().is_err());
    assert!(lease.binding().is_err());
    assert!(lease.credential().is_err());
    drop(lease);
    drop(selection);
    assert!(matches!(
        issuer.capture_service(&caller, ServicePurpose::InventoryRead),
        Err(ServiceUseError::Profile(
            graphit_auth::profile::ProfileError::Stale
        ))
    ));
}

#[test]
fn remote_inventory_without_active_account_denies_without_publication() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let rules = [rule(
        ENDPOINT,
        ServiceOrigin::Runtime,
        ServicePurpose::InventoryRead,
    )];
    let issuer = f.issuer(&rules, &[REMOTE], &ctx);
    let key = f.key();
    let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
    denied(issuer.capture_service(&caller, ServicePurpose::InventoryRead));
    assert!(!f.paths.root().join("auth.json").exists());
}

#[test]
fn at_sign_in_url_path_is_preserved_and_is_not_userinfo() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let endpoint = "https://broker.example/api@version";
    f.put(
        config(Some(endpoint), AiService::Disabled),
        data(Some(KEY), false),
        &ctx,
    );
    let rules = [rule(
        endpoint,
        ServiceOrigin::Runtime,
        ServicePurpose::InventoryRead,
    )];
    let issuer = f.issuer(&rules, &[REMOTE], &ctx);
    let key = f.key();
    let caller = issuer.authenticate_runtime(&key, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    let lease = selection.credential().unwrap();
    assert!(
        matches!(lease.binding().unwrap(), ServiceBindingView::BrokerInventory {
        endpoint: actual, ..
    } if actual == endpoint)
    );
}
