//! Shared concrete Auth/profile and wire fixture for the new Now surfaces. No Broker SQL proof.
use super::support;
use graphit_auth::{ExternalEffect, Permission};
use graphit_core::{brand::Brand, config::ConfigSnapshot, file_lock::Cancellation};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn network_config(f: &support::Fixture) -> ConfigSnapshot {
    use graphit_core::config::{ConfigDocument, ConfigKey, ConfigSource};
    ConfigSnapshot::new(
        f.paths.brand(),
        [ConfigDocument::new(
            f.paths.brand(),
            ConfigSource::Command,
            [(
                ConfigKey::new("broker.allow_loopback_http").unwrap(),
                json!(true),
            )],
        )
        .unwrap()],
    )
    .unwrap()
}
pub fn install_local_profile(f: &support::Fixture, endpoint: &str) {
    use graphit_auth::{profile::*, store::AuthStore};
    use graphit_core::{
        artifact_store::{CapturedPublicationScope, ScopeLimits, StoreOwner},
        auth_document::{AuthDocumentLimits, AuthDocumentStore},
    };
    let cancel = Cancellation::default();
    let original = original(&cancel);
    let StoreOwner::Installation { installation_id } = f.auth.installation().live.owner() else {
        panic!("fixture owner")
    };
    let captured = CapturedPublicationScope::installation(
        &f.paths,
        installation_id.clone(),
        f.auth
            .installation()
            .live
            .allowed_roots()
            .map(ToOwned::to_owned),
        ScopeLimits::new(16, 4096, 256).unwrap(),
        original.snapshot(),
    )
    .unwrap();
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            captured,
            AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
            original.snapshot(),
        )
        .unwrap(),
        ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
        &original,
    )
    .unwrap();
    let before = store.load(&original).unwrap();
    let written = store
        .mutate(before, &original, |doc, ctx| {
            doc.add_provider(
                "now".into(),
                ProviderConfig {
                    kind: ProviderKind::Local {
                        allow_aws_credential_chain: false,
                        broker: Some(BrokerService {
                            endpoint: endpoint.into(),
                            mcp_resource: None,
                            allow_anonymous: false,
                        }),
                    },
                    embedding: AiService::Broker,
                    rerank: AiService::Broker,
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
                provider_revision: doc.provider("now").unwrap().revision(),
                profile_revision: None,
            };
            doc.replace_profile_data(
                ProfileData {
                    name: "now".into(),
                    provider: "now".into(),
                    username: "fixture".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new(secrecy::SecretString::from("now-local-client"))),
                        broker_key: Some(Secret::new(secrecy::SecretString::from(
                            "now-protocol-fixture",
                        ))),
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: None,
                    }),
                },
                expected,
                1,
                ctx,
            )?;
            doc.activate("now", doc.revision(), ctx)?;
            Ok(())
        })
        .unwrap();
    assert!(written.failure().is_none());
}
pub struct ProtocolServer {
    pub endpoint: String,
    closed: tokio::sync::mpsc::Receiver<()>,
    stop: tokio_util::sync::CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl ProtocolServer {
    pub async fn start() -> Self {
        Self::start_with_terminal(false).await
    }
    pub async fn start_with_terminal(terminal: bool) -> Self {
        use axum::{
            Router,
            extract::ws::{Message, WebSocketUpgrade},
            routing::get,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (closed_send, closed) = tokio::sync::mpsc::channel(8);
        let app = Router::new().route(
            "/v1/realtime",
            get(move |headers: axum::http::HeaderMap, upgrade: WebSocketUpgrade| {
                let closed = closed_send.clone();
                async move {
                    assert_eq!(headers.get("authorization").unwrap(), "Bearer now-protocol-fixture");
                    upgrade.on_upgrade(move |mut socket| async move {
                        let Message::Text(first) = socket.recv().await.unwrap().unwrap() else {
                            panic!("first subscribe");
                        };
                        let first: Value = serde_json::from_str(&first).unwrap();
                        assert_eq!(first["op"], "subscribe");
                        socket.send(Message::Text(json!({"type":"welcome","heartbeat_timeout_ns":"4000000000"}).to_string().into())).await.unwrap();
                        let project = first["project_id"].clone();
                        let section = json!({"availability":"available","items":[],"total":0,"has_more":false,"error":null});
                        let now = json!({"project_id":project,"generated_at":"9007199254740993001","sections":{"tasks":section,"sessions":section,"memories":section,"knowledge":section}});
                        let mut instances = vec![
                            json!({"instance_id":"a","version":"main","now":now}),
                            json!({"instance_id":"b","version":"feature/β","now":now}),
                        ];
                        instances.retain(|value| {
                            first.get("instance_id").is_none_or(|filter| filter == &value["instance_id"])
                                && first.get("version").is_none_or(|filter| filter == &value["version"])
                        });
                        let full = json!({"type":"snapshot","project_id":project,"instances":instances});
                        if socket.send(Message::Text(full.to_string().into())).await.is_ok() {
                            let empty = json!({"type":"snapshot","project_id":project,"instances":[]});
                            let _ = socket.send(Message::Text(empty.to_string().into())).await;
                        }
                        if terminal {
                            let error = json!({"type":"error","code":"unauthenticated"});
                            let _ = socket.send(Message::Text(error.to_string().into())).await;
                        }
                        while let Some(Ok(message)) = socket.recv().await {
                            if matches!(message, Message::Close(_)) {
                                break;
                            }
                        }
                        drop(socket);
                        let _ = closed.send(()).await;
                    })
                }
            }),
        );
        let stop = tokio_util::sync::CancellationToken::new();
        let stopping = stop.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(stopping.cancelled_owned())
                .await
                .unwrap();
        });
        Self {
            endpoint,
            closed,
            stop,
            task,
        }
    }
    pub async fn closed(&mut self) {
        tokio::time::timeout(Duration::from_secs(5), self.closed.recv())
            .await
            .unwrap()
            .unwrap();
    }
    pub async fn shutdown(self) {
        self.stop.cancel();
        self.task.await.unwrap();
    }
}
pub fn remote_fixture(endpoint: &str) -> support::Fixture {
    use graphit_auth::{
        CredentialSource, RemoteServiceBindingInput, ServiceOrigin, ServicePurpose,
        ServiceRuleInput,
    };
    let rules = [ServiceRuleInput::Remote {
        origin: ServiceOrigin::Runtime,
        binding: RemoteServiceBindingInput::Broker {
            provider: "now",
            purpose: ServicePurpose::PresenceRead,
            endpoint,
            resource: None,
        },
        credential: CredentialSource::LocalBrokerKey,
        incoming: None,
    }];
    let mut f = support::fixture_with_projects_and_rules(
        &[Permission::External(ExternalEffect::RemoteRequest)],
        Brand::default(),
        NativeBinding::Codex,
        &["project"],
        &rules,
    );
    install_local_profile(&f, endpoint);
    let config = network_config(&f);
    f.server.attach_now(None, &config).unwrap();
    f
}

pub fn content() -> ContentLimits {
    ContentLimits {
        max_input_bytes: 65536,
        max_output_bytes: 65536,
        max_depth: 32,
        max_nodes: 4096,
        max_scalar_bytes: 4096,
        max_collection_entries: 64,
    }
}
pub fn original(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(content(), cancel, Instant::now() + Duration::from_secs(30)).unwrap()
}
