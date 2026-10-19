use super::*;
use clap::Parser;
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ServiceAdmissionLimits,
    ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy, profile::ProfileLimits,
};
use graphit_core::{
    artifact_store::ScopeLimits,
    auth_document::{AuthDocumentLimits, publication::AuthPublicationEffect},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::BootstrapLimits,
    project_paths::InstallationPaths,
    runtime_key::RuntimeKeyLimits,
};
use graphit_daemon::auth_startup::{DaemonAuth, StartupPolicy};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}
pub(super) fn command(arguments: &[&str]) -> Command {
    Arguments::try_parse_from(std::iter::once("graphit").chain(arguments.iter().copied()))
        .unwrap()
        .command
}
pub(super) fn request(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
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
fn policy(admin: bool) -> StartupPolicy {
    let administrative = if admin {
        vec![Permission::ManageAuthProfiles]
    } else {
        vec![]
    };
    let origin = |administrative| SourceAuthorityPolicy {
        ordinary: &[],
        administrative,
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    StartupPolicy {
        bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
        runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
        scope: ScopeLimits::new(16, 4096, 128).unwrap(),
        document: AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
        profiles: ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
        authority: IssuerAuthorityPolicy::new(
            origin(&administrative),
            origin(&[]),
            IssuerLimits {
                max_permissions: 64,
                max_scope_bytes: 65536,
                max_parent_bearer_bytes: 1024,
            },
            ServiceAuthorityInput {
                rules: &[],
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
pub(super) struct Fixture {
    _directory: tempfile::TempDir,
    paths: InstallationPaths,
    pub(super) auth: DaemonAuth,
}
impl Fixture {
    pub(super) fn new(admin: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let cancel = Cancellation::default();
        let original = request(&cancel, Instant::now() + Duration::from_secs(30));
        let auth = DaemonAuth::start(&paths, &[], policy(admin), &original).unwrap();
        Self {
            _directory: directory,
            paths,
            auth,
        }
    }
    fn run(&self, arguments: &[&str], now: i64) -> Result<CommandResult, CommandError> {
        let cancel = Cancellation::default();
        let original = request(&cancel, Instant::now() + Duration::from_secs(30));
        let key = self.auth.runtime().check(original.snapshot()).unwrap();
        let raw = std::str::from_utf8(key.secret(original.snapshot()).unwrap()).unwrap();
        let caller = self
            .auth
            .issuer()
            .authenticate_runtime(raw, &original)
            .unwrap();
        let session = self.auth.issuer().bootstrap_admin(&caller).unwrap();
        let parent = self.auth.issuer().admin_parent(&caller, &session).unwrap();
        let manager = NativeProfileManager::new(self.auth.issuer());
        execute(
            command(arguments),
            &manager,
            ProfileManagerAuthority::Admin(&parent),
            now,
        )
    }
    pub(super) fn read(&self, arguments: &[&str]) -> Value {
        match self.run(arguments, 20).unwrap() {
            CommandResult::Read(value) => value,
            CommandResult::Publication(_) => panic!("expected read"),
        }
    }
    pub(super) fn publish(&self, arguments: &[&str], now: i64) {
        match self.run(arguments, now).unwrap() {
            CommandResult::Publication(publication) => {
                assert_eq!(publication.effect(), AuthPublicationEffect::Applied);
                assert!(publication.failure().is_none());
                assert!(publication.cleanup_failure().is_none());
                assert!(publication.into_pending_cleanup().is_none());
            }
            CommandResult::Read(_) => panic!("expected publication"),
        }
    }
    fn authenticate_local(&self, key: &str) -> Result<(), graphit_auth::AuthError> {
        let cancel = Cancellation::default();
        let original = request(&cancel, Instant::now() + Duration::from_secs(30));
        self.auth
            .issuer()
            .authenticate_local(key, &original)
            .map(|_| ())
    }
    pub(super) fn bytes(&self) -> Option<Vec<u8>> {
        std::fs::read(self.paths.global_path(&["auth.json"]).unwrap()).ok()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let cancel = Cancellation::default();
        let original = request(&cancel, Instant::now() + Duration::from_secs(30));
        let outcome = self.auth.remove_runtime(original.snapshot());
        assert!(outcome.failure().is_none());
        assert!(outcome.cleanup_failure().is_none());
    }
}

#[test]
fn local_login_account_selection_and_logout_keep_credentials_private() {
    let f = Fixture::new(true);
    f.publish(&["provider", "add", "workstation", "--type", "local"], 1);
    for (name, now) in [("alice", 2), ("bob", 3)] {
        let key = if name == "alice" {
            "secret-cli-canary"
        } else {
            "other-secret-canary"
        };
        f.publish(
            &[
                "login",
                "--provider",
                "workstation",
                "--profile",
                name,
                "--username",
                name,
                "--mcp-key",
                key,
                "--organization",
                "acme",
                "--team",
                "platform",
            ],
            now,
        );
    }
    let accounts = f.read(&["account", "list"]);
    assert_eq!(accounts["active"], "bob");
    assert_eq!(accounts["profiles"].as_array().unwrap().len(), 2);
    assert!(!accounts.to_string().contains("secret-cli-canary"));
    assert!(!accounts.to_string().contains("mcp_key"));
    f.publish(&["account", "use", "alice"], 4);
    assert_eq!(f.read(&["account", "show"])["profile"]["name"], "alice");
    f.publish(&["logout"], 5);
    assert!(f.read(&["account", "show"])["profile"].is_null());
    assert_eq!(
        f.read(&["account", "list"])["profiles"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        !f.bytes()
            .unwrap()
            .windows(b"secret-cli-canary".len())
            .any(|v| v == b"secret-cli-canary")
    );
}
#[test]
fn provider_creation_defaults_match_kind_and_preserve_active_local_profile() {
    let f = Fixture::new(true);
    f.publish(&["provider", "add", "local", "--type", "local"], 1);
    let local = f.read(&["provider", "show", "local"]);
    assert_eq!(local["config"]["s3"]["source"], "login");
    assert_eq!(local["config"]["embedding"]["mode"], "local");
    assert_eq!(local["config"]["rerank"]["mode"], "local");
    f.publish(
        &[
            "login",
            "--provider",
            "local",
            "--profile",
            "workstation",
            "--username",
            "workstation",
            "--mcp-key",
            "provider-default-canary",
        ],
        2,
    );
    f.publish(
        &[
            "provider",
            "add",
            "broker-local",
            "--type",
            "broker",
            "--broker-endpoint",
            "http://127.0.0.1:34461",
            "--mcp-resource",
            "http://127.0.0.1:34461/v1",
            "--embedding-mode",
            "disabled",
            "--rerank-mode",
            "disabled",
        ],
        3,
    );
    let broker = f.read(&["provider", "show", "broker-local"]);
    assert_eq!(broker["config"]["type"], "broker");
    assert_eq!(broker["config"]["s3"]["source"], "broker");
    assert!(broker["config"]["s3"]["bucket"].is_null());
    assert_eq!(broker["config"]["embedding"]["mode"], "disabled");
    assert_eq!(broker["config"]["rerank"]["mode"], "disabled");
    f.publish(
        &[
            "provider",
            "add",
            "broker-defaults",
            "--type",
            "broker",
            "--broker-endpoint",
            "http://127.0.0.1:34461",
        ],
        4,
    );
    let defaults = f.read(&["provider", "show", "broker-defaults"]);
    assert_eq!(defaults["config"]["s3"]["source"], "broker");
    assert_eq!(defaults["config"]["embedding"]["mode"], "broker");
    assert_eq!(defaults["config"]["rerank"]["mode"], "broker");
    assert_eq!(f.read(&["account", "list"])["active"], "workstation");
    assert!(f.authenticate_local("provider-default-canary").is_ok());
}

#[test]
fn broker_provider_rejects_local_direct_and_device_options_without_publication() {
    let f = Fixture::new(true);
    for purpose in ["--embedding-mode", "--rerank-mode"] {
        for mode in ["local", "direct"] {
            let prior = f.bytes();
            assert!(
                f.run(
                    &[
                        "provider",
                        "add",
                        "invalid-ai",
                        "--type",
                        "broker",
                        "--broker-endpoint",
                        "http://127.0.0.1:34461",
                        purpose,
                        mode,
                    ],
                    1
                )
                .is_err()
            );
            assert_eq!(f.bytes(), prior);
        }
    }
    f.publish(
        &[
            "provider",
            "add",
            "managed",
            "--type",
            "broker",
            "--broker-endpoint",
            "http://127.0.0.1:34461",
        ],
        1,
    );
    for options in [
        ["--embedding-mode", "local"],
        ["--rerank-mode", "local"],
        ["--embedding-device", "cpu"],
        ["--rerank-device", "cpu"],
    ] {
        let prior = f.bytes();
        assert!(
            f.run(
                &["provider", "update", "managed", options[0], options[1]],
                2
            )
            .is_err()
        );
        assert_eq!(f.bytes(), prior);
    }
}

#[test]
fn provider_updates_preserve_sources_and_require_explicit_kind_transition() {
    let f = Fixture::new(true);
    f.publish(
        &[
            "provider",
            "add",
            "storage",
            "--type",
            "local",
            "--allow-aws-credential-chain",
            "--s3-credential-source",
            "aws-chain",
            "--s3-bucket",
            "data",
        ],
        1,
    );
    f.publish(
        &["provider", "update", "storage", "--rerank-mode", "disabled"],
        2,
    );
    assert_eq!(
        f.read(&["provider", "show", "storage"])["config"]["s3"]["source"],
        "aws-chain"
    );
    let prior = f.bytes();
    let transition = [
        "provider",
        "update",
        "storage",
        "--type",
        "broker",
        "--broker-endpoint",
        "http://127.0.0.1:34461",
    ];
    assert!(matches!(
        f.run(&transition, 3),
        Err(CommandError::Profile(_))
    ));
    assert_eq!(f.bytes(), prior);
    let explicit: Vec<&str> = transition
        .into_iter()
        .chain(["--s3-credential-source", "broker"])
        .collect();
    f.publish(&explicit, 3);
    f.publish(
        &[
            "provider",
            "update",
            "storage",
            "--embedding-mode",
            "disabled",
        ],
        4,
    );
    let broker = f.read(&["provider", "show", "storage"]);
    assert_eq!(broker["config"]["s3"]["source"], "broker");
    assert_eq!(broker["config"]["s3"]["bucket"], "data");
    let prior = f.bytes();
    assert!(matches!(
        f.run(&["provider", "update", "storage", "--clear-s3"], 5),
        Err(CommandError::Profile(_))
    ));
    assert_eq!(f.bytes(), prior);
    f.publish(
        &[
            "provider",
            "update",
            "storage",
            "--type",
            "local",
            "--clear-broker",
            "--s3-credential-source",
            "login",
        ],
        5,
    );
    assert_eq!(
        f.read(&["provider", "show", "storage"])["config"]["s3"]["source"],
        "login"
    );
}

#[test]
fn provider_creation_rejects_explicit_incompatible_source_and_nonloopback_http() {
    let f = Fixture::new(true);
    let prior = f.bytes();
    for args in [
        vec![
            "provider",
            "add",
            "invalid-source",
            "--type",
            "broker",
            "--broker-endpoint",
            "http://127.0.0.1:34461",
            "--s3-credential-source",
            "login",
        ],
        vec![
            "provider",
            "add",
            "invalid-url",
            "--type",
            "broker",
            "--broker-endpoint",
            "http://broker.example",
        ],
    ] {
        assert!(matches!(f.run(&args, 1), Err(CommandError::Profile(_))));
        assert_eq!(f.bytes(), prior);
    }
}

#[test]
fn provider_update_preserves_omitted_settings_and_invalidates_dependent_login() {
    let f = Fixture::new(true);
    f.publish(
        &[
            "provider",
            "add",
            "local",
            "--type",
            "local",
            "--embedding-device",
            "cuda",
            "--embedding-device-id",
            "2",
            "--s3-bucket",
            "data",
            "--s3-region",
            "us-east-1",
        ],
        1,
    );
    f.publish(
        &[
            "login",
            "--provider",
            "local",
            "--profile",
            "alice",
            "--username",
            "alice",
            "--mcp-key",
            "local-profile-canary",
            "--s3-access-key",
            "local-access-canary",
            "--s3-secret-key",
            "local-s3-secret-canary",
            "--s3-expires-at",
            "1000",
        ],
        2,
    );
    assert!(
        !f.read(&["account", "show"])
            .to_string()
            .contains("local-s3-secret-canary")
    );
    assert!(f.authenticate_local("local-profile-canary").is_ok());
    f.publish(
        &["provider", "update", "local", "--rerank-mode", "disabled"],
        3,
    );
    let config = &f.read(&["provider", "show", "local"])["config"];
    assert_eq!(config["embedding"]["device_id"], 2);
    assert_eq!(config["s3"]["bucket"], "data");
    assert_eq!(config["rerank"]["mode"], "disabled");
    assert_eq!(f.read(&["account", "list"])["active"], "alice");
    assert!(matches!(
        f.authenticate_local("local-profile-canary"),
        Err(graphit_auth::AuthError::Denied)
    ));
}
#[test]
fn provider_removal_requires_explicit_intent_and_actual_cascade() {
    assert!(Arguments::try_parse_from(["graphit", "provider", "remove", "local"]).is_err());
    let f = Fixture::new(true);
    f.publish(&["provider", "add", "local", "--type", "local"], 1);
    f.publish(
        &[
            "login",
            "--provider",
            "local",
            "--profile",
            "alice",
            "--username",
            "alice",
        ],
        2,
    );
    let before = f.bytes();
    assert!(f.run(&["provider", "remove", "local", "--yes"], 3).is_err());
    assert_eq!(f.bytes(), before);
    f.publish(&["provider", "remove", "local", "--yes", "--cascade"], 4);
    assert!(
        f.read(&["provider", "list"])["providers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        f.read(&["account", "list"])["profiles"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn expected_revision_conflict_does_not_write() {
    let f = Fixture::new(true);
    f.publish(&["provider", "add", "local", "--type", "local"], 1);
    let before = f.bytes();
    assert!(matches!(
        f.run(
            &[
                "provider",
                "update",
                "local",
                "--expected-revision",
                "1",
                "--embedding-mode",
                "disabled"
            ],
            2
        ),
        Err(CommandError::Conflict)
    ));
    assert_eq!(f.bytes(), before);
}
#[test]
fn runtime_identity_without_administrative_permission_is_denied() {
    let f = Fixture::new(false);
    let before = f.bytes();
    assert!(matches!(
        f.run(&["provider", "add", "local", "--type", "local"], 1),
        Err(CommandError::Profile(_))
    ));
    assert_eq!(f.bytes(), before);
}
#[test]
fn cancellation_and_deadline_preserve_the_captured_original() {
    let f = Fixture::new(true);
    for canceled in [true, false] {
        let cancel = Cancellation::default();
        let original = request(&cancel, Instant::now() + Duration::from_millis(100));
        let key = f.auth.runtime().check(original.snapshot()).unwrap();
        let raw = std::str::from_utf8(key.secret(original.snapshot()).unwrap()).unwrap();
        let caller = f
            .auth
            .issuer()
            .authenticate_runtime(raw, &original)
            .unwrap();
        let session = f.auth.issuer().bootstrap_admin(&caller).unwrap();
        let parent = f.auth.issuer().admin_parent(&caller, &session).unwrap();
        let manager = NativeProfileManager::new(f.auth.issuer());
        if canceled {
            cancel.cancel();
        } else {
            std::thread::sleep(Duration::from_millis(120));
        }
        let before = f.bytes();
        assert!(matches!(
            execute(
                command(&["provider", "add", "local", "--type", "local"]),
                &manager,
                ProfileManagerAuthority::Admin(&parent),
                1
            ),
            Err(CommandError::Profile(_))
        ));
        assert_eq!(f.bytes(), before);
    }
}
#[test]
fn conflicting_and_unknown_arguments_are_errors() {
    for args in [
        vec![
            "login",
            "--profile",
            "x",
            "--provider",
            "local",
            "--anonymous",
            "--username",
            "x",
        ],
        vec![
            "login",
            "--profile",
            "x",
            "--provider",
            "local",
            "--username",
            "x",
            "--s3-secret-key",
            "secret",
        ],
        vec![
            "provider",
            "add",
            "x",
            "--type",
            "local",
            "--clear-s3",
            "--s3-bucket",
            "data",
        ],
        vec!["account", "use", "x", "--unknown"],
    ] {
        assert!(Arguments::try_parse_from(std::iter::once("graphit").chain(args)).is_err());
    }
    let f = Fixture::new(true);
    assert!(matches!(
        f.run(
            &[
                "provider",
                "add",
                "direct",
                "--type",
                "local",
                "--embedding-mode",
                "direct",
                "--embedding-device",
                "cpu"
            ],
            1
        ),
        Err(CommandError::InvalidInput(_))
    ));
    assert!(f.bytes().is_none());
}
#[test]
fn provider_endpoint_projection_does_not_expose_url_credentials() {
    let value = provider(&ProviderConfig {
        kind: ProviderKind::Broker(graphit_auth::profile::BrokerService {
            endpoint: "https://user:private@example.com/api?token=private#private".into(),
            mcp_resource: Some("https://example.com/mcp?key=private".into()),
            allow_anonymous: false,
        }),
        embedding: AiService::Broker,
        rerank: AiService::Broker,
        s3: graphit_auth::profile::S3Location {
            bucket: None,
            region: None,
            endpoint: None,
            prefix: None,
            source: graphit_auth::profile::S3Source::BrokerScope,
        },
    });
    assert_eq!(value["broker"]["endpoint"], "https://example.com/api");
    assert!(!value.to_string().contains("private"));
}

#[test]
fn literal_ai_options_preserve_omission_and_require_explicit_clearing() {
    let f = Fixture::new(true);
    f.publish(
        &[
            "provider",
            "add",
            "openai",
            "--type",
            "local",
            "--embedding-mode",
            "direct",
            "--embedding-protocol",
            "openai",
            "--embedding-endpoint",
            "https://ai.example/v1/embeddings",
            "--embedding-model",
            "text-embedding-3-small",
            "--embedding-model-revision",
            "fixture-deployment-2026-10",
            "--embedding-recipe",
            "fixture-vector-v1",
            "--embedding-dimensions",
            "512",
            "--embedding-request-dimensions",
            "512",
        ],
        1,
    );
    f.publish(
        &["provider", "update", "openai", "--rerank-mode", "disabled"],
        2,
    );
    let config = f.read(&["provider", "show", "openai"]);
    assert_eq!(
        config["config"]["embedding"]["model_revision"],
        "fixture-deployment-2026-10"
    );
    assert_eq!(config["config"]["embedding"]["recipe"], "fixture-vector-v1");
    assert_eq!(
        f.read(&["provider", "show", "openai"])["config"]["embedding"]["request_dimensions"],
        512
    );
    let prior = f.bytes();
    assert!(
        f.run(
            &[
                "provider",
                "update",
                "openai",
                "--embedding-protocol",
                "openai-compatible"
            ],
            3
        )
        .is_err()
    );
    assert_eq!(f.bytes(), prior);
    f.publish(
        &[
            "provider",
            "update",
            "openai",
            "--embedding-protocol",
            "openai-compatible",
            "--embedding-clear-request-dimensions",
        ],
        3,
    );
    assert!(
        f.read(&["provider", "show", "openai"])["config"]["embedding"]["request_dimensions"]
            .is_null()
    );
    f.publish(
        &[
            "provider",
            "add",
            "google",
            "--type",
            "local",
            "--embedding-mode",
            "direct",
            "--embedding-protocol",
            "google-gemini-2",
            "--embedding-endpoint",
            "https://ai.example/v1/models/retrieval:embedContent",
            "--embedding-batch-endpoint",
            "https://ai.example/v1/models/retrieval:batchEmbedContents",
            "--embedding-model",
            "models/retrieval",
            "--embedding-model-revision",
            "fixture-deployment-2026-10",
            "--embedding-recipe",
            "fixture-vector-v1",
            "--embedding-dimensions",
            "768",
        ],
        4,
    );
    f.publish(
        &["provider", "update", "google", "--rerank-mode", "disabled"],
        5,
    );
    assert_eq!(
        f.read(&["provider", "show", "google"])["config"]["embedding"]["batch_endpoint"],
        "https://ai.example/v1/models/retrieval:batchEmbedContents"
    );
    let prior = f.bytes();
    assert!(
        f.run(
            &[
                "provider",
                "update",
                "google",
                "--embedding-clear-batch-endpoint"
            ],
            6
        )
        .is_err()
    );
    assert_eq!(f.bytes(), prior);
    assert!(
        Arguments::try_parse_from([
            "graphit",
            "provider",
            "update",
            "google",
            "--embedding-protocol",
            "google"
        ])
        .is_err()
    );
    assert!(
        Arguments::try_parse_from([
            "graphit",
            "provider",
            "update",
            "openai",
            "--embedding-request-dimensions",
            "128",
            "--embedding-clear-request-dimensions"
        ])
        .is_err()
    );
}

#[test]
fn direct_identity_options_are_required_and_local_device_flags_are_canonical() {
    let f = Fixture::new(true);
    let complete = vec![
        "provider",
        "add",
        "direct",
        "--type",
        "local",
        "--embedding-mode",
        "direct",
        "--embedding-protocol",
        "openai",
        "--embedding-endpoint",
        "https://ai.example/v1/embeddings",
        "--embedding-model",
        "embedding-model",
        "--embedding-dimensions",
        "768",
        "--embedding-model-revision",
        "deployment-2026-10",
        "--embedding-recipe",
        "retrieval-v1",
    ];
    for omitted in ["--embedding-model-revision", "--embedding-recipe"] {
        let mut arguments = complete.clone();
        let index = arguments
            .iter()
            .position(|option| *option == omitted)
            .unwrap();
        arguments.drain(index..index + 2);
        assert!(matches!(
            f.run(&arguments, 1),
            Err(CommandError::InvalidInput(_))
        ));
        assert!(f.bytes().is_none());
    }
    for option in ["--embedding-device", "--rerank-device"] {
        for (literal, expected) in [
            ("cpu", graphit_auth::profile::LocalDevice::Cpu),
            ("cuda", graphit_auth::profile::LocalDevice::Cuda),
            ("metal", graphit_auth::profile::LocalDevice::Metal),
            ("auto", graphit_auth::profile::LocalDevice::Auto),
        ] {
            let Command::Provider(arguments) = command(&[
                "provider", "add", "local", "--type", "local", option, literal,
            ]) else {
                panic!("expected provider add");
            };
            let ProviderCommand::Add { fields, kind, .. } = arguments.command else {
                panic!("expected provider add");
            };
            let config = fields.apply(Some(kind), None).unwrap();
            let execution = if option == "--embedding-device" {
                &config.embedding
            } else {
                &config.rerank
            };
            assert!(
                matches!(execution, AiService::Local(execution) if execution.device == expected && execution.device_id == 0)
            );
        }
        assert!(
            Arguments::try_parse_from([
                "graphit", "provider", "add", "local", "--type", "local", option, "coreml"
            ])
            .is_err()
        );
    }
    for option in ["--embedding-onnx-device", "--rerank-onnx-device"] {
        assert!(
            Arguments::try_parse_from([
                "graphit", "provider", "add", "local", "--type", "local", option, "cpu"
            ])
            .is_err()
        );
    }
    let config = ProviderFields::default()
        .apply(Some(ProviderType::Local), None)
        .unwrap();
    for service in [&config.embedding, &config.rerank] {
        assert!(
            matches!(service, AiService::Local(execution) if execution.device == graphit_auth::profile::LocalDevice::Cpu && execution.device_id == 0)
        );
    }
}

#[test]
fn direct_rerank_admits_native_protocols_and_rejects_embedding_only_configuration() {
    let f = Fixture::new(true);
    let args = |protocol| {
        vec![
            "provider",
            "add",
            "rerank",
            "--type",
            "local",
            "--rerank-mode",
            "direct",
            "--rerank-protocol",
            protocol,
            "--rerank-endpoint",
            "https://ai.example/rerank",
            "--rerank-model",
            "reranker",
            "--rerank-model-revision",
            "deployment-1",
            "--rerank-recipe",
            "native-logit-v1",
            "--rerank-dimensions",
            "0",
        ]
    };
    for protocol in [
        "openai",
        "openai-compatible",
        "google-gemini-1",
        "google-gemini-2",
    ] {
        assert!(matches!(
            f.run(&args(protocol), 1),
            Err(CommandError::InvalidInput(_))
        ));
        assert!(f.bytes().is_none());
    }
    f.publish(&args("cohere-v2"), 1);
    for protocol in [
        "cohere",
        "cohere-v2",
        "voyage",
        "voyage-v1",
        "jina",
        "jina-v1",
    ] {
        f.publish(
            &[
                "provider",
                "update",
                "rerank",
                "--rerank-protocol",
                protocol,
            ],
            2,
        );
        assert_eq!(
            f.read(&["provider", "show", "rerank"])["config"]["rerank"]["protocol"],
            protocol
        );
    }
    let prior = f.bytes();
    for protocol in [
        "openai",
        "openai-compatible",
        "google-gemini-1",
        "google-gemini-2",
    ] {
        assert!(
            f.run(
                &[
                    "provider",
                    "update",
                    "rerank",
                    "--rerank-protocol",
                    protocol
                ],
                3
            )
            .is_err()
        );
        assert_eq!(f.bytes(), prior);
    }
    for (flag, value) in [
        ("--rerank-request-dimensions", Some("128")),
        ("--rerank-clear-request-dimensions", None),
        ("--rerank-batch-endpoint", Some("https://ai.example/batch")),
        ("--rerank-clear-batch-endpoint", None),
    ] {
        let mut arguments = vec!["graphit", "provider", "update", "rerank", flag];
        arguments.extend(value);
        match Arguments::try_parse_from(arguments) {
            Err(error) => assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument),
            Ok(_) => panic!("removed rerank option was accepted: {flag}"),
        }
    }
}
