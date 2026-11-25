//! HTTP admission must not wait for the first remote Now welcome.
use super::*;
use axum::{Router, extract::State, http::StatusCode};
use graphit_auth::{
    profile::{
        AiService, BrokerService, LocalCredentials, ProfileCredentials, ProfileData,
        ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
        Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    config::{ConfigDocument, ConfigSource},
    project_identity::{ProjectIdentityStore, ProjectMetadata},
};
use secrecy::SecretString;
use serde_json::json;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn fixture(endpoint: &str) -> Result<(tempfile::TempDir, ForegroundSource, Zeroizing<String>)> {
    fixture_projects(endpoint, 1)
}

fn fixture_projects(
    endpoint: &str,
    count: usize,
) -> Result<(tempfile::TempDir, ForegroundSource, Zeroizing<String>)> {
    let directory = tempfile::tempdir()?;
    let brand = Brand::default();
    let installation = InstallationPaths::resolve(
        brand.clone(),
        directory.path(),
        None,
        Some(&directory.path().join("installation")),
    )?;
    let config = ConfigSnapshot::new(
        &brand,
        [ConfigDocument::new(
            &brand,
            ConfigSource::Command,
            [
                (ConfigKey::new("broker.allow_loopback_http")?, json!(true)),
                (ConfigKey::new("modules.ast")?, json!(false)),
                (ConfigKey::new("modules.knowledge")?, json!(false)),
                (ConfigKey::new("modules.memory")?, json!(false)),
            ],
        )?],
    )?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancellation = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancellation)?;
    original.check()?;
    drop(graphit_fs::create_private_directory(installation.root())?);
    graphit_core::installation_identity::InstallationIdentityStore::new(&installation)?
        .ensure(Duration::from_secs(5), &cancellation)?;
    let store = AuthStore::new(
        AuthDocumentStore::new(
            installation.root().to_owned(),
            AuthDocumentLimits::new(1048576)?,
            original.snapshot(),
        )?,
        ProfileLimits {
            bytes: 1048576,
            nodes: 65536,
            scalar_bytes: 65536,
            collection_entries: 1024,
            providers: 16,
            profiles: 16,
        },
        &original,
    )?;
    let before = store.load(&original)?;
    let published = store.mutate(before, &original, |doc, request| {
        doc.add_provider(
            "local".into(),
            ProviderConfig {
                kind: ProviderKind::Local {
                    allow_aws_credential_chain: false,
                    broker: Some(BrokerService {
                        endpoint: endpoint.into(),
                        mcp_resource: None,
                        allow_anonymous: false,
                    }),
                },
                embedding: AiService::Disabled,
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
            request,
        )?;
        let expected = ProfileExpectation {
            document_revision: doc.revision(),
            provider_revision: doc.provider("local").unwrap().revision(),
            profile_revision: None,
        };
        doc.replace_profile_data(
            ProfileData {
                name: "local".into(),
                provider: "local".into(),
                username: "startup fixture".into(),
                organization: None,
                teams: vec![],
                credentials: ProfileCredentials::Local(LocalCredentials {
                    anonymous: false,
                    mcp_key: Some(Secret::new(SecretString::from("local-startup-mcp"))),
                    broker_key: Some(Secret::new(SecretString::from("local-startup-broker"))),
                    embedding_api_key: None,
                    rerank_api_key: None,
                    s3: None,
                }),
            },
            expected,
            1,
            request,
        )?;
        doc.activate("local", doc.revision(), request)?;
        Ok(())
    })?;
    assert_eq!(
        published,
        graphit_core::auth_document::publication::AuthPublicationEffect::Applied
    );
    drop(store);
    let mut projects = Vec::new();
    for index in 0..count {
        let name = if index == 0 {
            "project".to_owned()
        } else {
            format!("project-{index}")
        };
        let project_root = directory.path().join(name);
        std::fs::create_dir(&project_root)?;
        let project = ProjectPaths::at(&project_root, &installation)?;
        ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &cancellation)?
            .ensure(&ProjectMetadata::new("startup")?)?;
        projects.push(project);
    }
    let inputs = projects
        .iter()
        .map(|project| ProjectBootstrapInput {
            project,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        })
        .collect::<Vec<_>>();
    let provider = active_provider(&installation, &config, &original)?;
    let owner = ForegroundSource::start_with_inventory(
        &installation,
        &inputs,
        0,
        &config,
        NativeBinding::Codex,
        &original,
        provider.as_ref(),
    )?;
    assert!(
        owner.now.is_some(),
        "the actual Now publisher must be configured"
    );
    let key = owner.auth.runtime().check(original.snapshot())?;
    let raw = Zeroizing::new(std::str::from_utf8(key.secret(original.snapshot())?)?.to_owned());
    drop(key);
    Ok((directory, owner, raw))
}

async fn pending_startup(reject_first: bool) -> Result {
    let (observed, mut observation) = watch::channel(false);
    let (release, wait) = watch::channel(false);
    let remote = Router::new()
        .fallback(
            async move |State((observed, mut wait)): State<(
                watch::Sender<bool>,
                watch::Receiver<bool>,
            )>| {
                observed.send_replace(true);
                let _ = wait.wait_for(|released| *released).await;
                (
                    StatusCode::UNAUTHORIZED,
                    [(axum::http::header::WWW_AUTHENTICATE, "Bearer")],
                    axum::Json(json!({"error":{"code":"unauthenticated"}})),
                )
            },
        )
        .with_state((observed, wait));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let remote_stop = CancellationToken::new();
    let remote_cancel = remote_stop.clone();
    let mut remote_job = tokio::task::JoinSet::new();
    remote_job.spawn(async move {
        axum::serve(listener, remote)
            .with_graceful_shutdown(remote_cancel.cancelled_owned())
            .await
    });
    let (_directory, owner, raw) = fixture(&endpoint)?;
    let address = owner.address().unwrap();
    let selected = &owner.projects.as_ref().unwrap()[0];
    let project_command = crate::foreground_project_control::ProjectCommand::MaintainCheckout {
        project_id: selected.1,
        project_dir: graphit_core::native_path::NativePath::new(selected.0.root().to_owned())?,
        version: selected.2.version().to_owned(),
        detached_base: String::new(),
        select: true,
    };
    let cancellation = Cancellation::default();
    let stop = cancellation.clone();
    let mut terminal = owner.now_terminal_effect.subscribe();
    let mut running = tokio::task::JoinSet::new();
    running.spawn(async move { owner.run(&stop).await });
    let reached = tokio::time::timeout(
        Duration::from_secs(10),
        observation.wait_for(|started| *started),
    )
    .await
    .is_ok_and(|observed| observed.is_ok());
    // A real pending remote operation has not returned any Welcome or snapshot.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let reply = client
        .post(format!("http://{address}/admin/daemon"))
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(raw.as_str())
        .json(&json!({"operation":"status"}))
        .send()
        .await;
    let denied = client
        .post(format!("http://{address}/admin/daemon"))
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth("invalid")
        .json(&json!({"operation":"status"}))
        .send()
        .await;
    // Prove the complete HTTP reply while Now is still pending. Reading only
    // headers here and the body after shutdown races the owned HTTP task's exit.
    let project_reply = async {
        let reply = client
            .post(format!("http://{address}/admin/projects"))
            .header("x-graphit-auth-origin", "runtime")
            .bearer_auth(raw.as_str())
            .json(&project_command)
            .send()
            .await?;
        let status = reply.status();
        let body = reply.json::<serde_json::Value>().await?;
        Ok::<_, reqwest::Error>((status, body))
    }
    .await;
    assert!(
        running.try_join_next().is_none(),
        "Now startup must still be pending"
    );
    if reject_first {
        release.send_replace(true);
        // The Source has joined and retired the failed publisher. A remote
        // response alone does not order that transition before service shutdown.
        tokio::time::timeout(Duration::from_secs(10), terminal.changed()).await??;
        // HTTP 401 is terminal for this publisher; Source retains its local
        // listener and unrelated owners. Typed Now errors are covered by the
        // publisher contract; this consumer checks isolation and owned shutdown.
        let after_denial = client
            .post(format!("http://{address}/admin/daemon"))
            .header("x-graphit-auth-origin", "runtime")
            .bearer_auth(raw.as_str())
            .json(&json!({"operation":"status"}))
            .send()
            .await?;
        assert_eq!(after_denial.status(), StatusCode::OK);
        let _: serde_json::Value = after_denial.json().await?;
        assert!(
            running.try_join_next().is_none(),
            "publisher denial must not stop the local Source listener"
        );
    }
    cancellation.cancel();
    let result = tokio::time::timeout(Duration::from_secs(10), running.join_next()).await;
    release.send_replace(true);
    remote_stop.cancel();
    let remote_result = remote_job.join_next().await;
    assert!(
        reached,
        "the actual first Now remote operation must be observed"
    );
    assert_eq!(reply?.status(), StatusCode::OK);
    assert_eq!(denied?.status(), StatusCode::UNAUTHORIZED);
    let (project_status, project_body) = project_reply?;
    assert_eq!(project_status, StatusCode::OK);
    assert_eq!(project_body["phase"], "unchanged");
    result?.unwrap()??;
    assert!(remote_result.unwrap()?.is_ok());
    assert!(
        tokio::net::TcpStream::connect(address).await.is_err(),
        "the owned HTTP listener must be closed after explicit cancellation"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authenticated_http_serves_during_first_now_startup_and_cancel_joins() -> Result {
    pending_startup(false).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_now_401_retires_publisher_and_keeps_http_until_cancel() -> Result {
    pending_startup(true).await
}

#[derive(Clone)]
struct InventoryDeadlineBroker {
    target: String,
    healthy: Arc<std::sync::atomic::AtomicBool>,
    registrations: Arc<std::sync::atomic::AtomicUsize>,
    target_connections: Arc<std::sync::atomic::AtomicUsize>,
    sibling_connections: Arc<std::sync::atomic::AtomicUsize>,
    disconnect: watch::Receiver<bool>,
    probe: watch::Receiver<u64>,
    events: tokio::sync::mpsc::Sender<PublisherObservation>,
}

#[derive(Debug)]
enum PublisherObservation {
    Ready { target: bool, connection: usize },
    Pong { target: bool, nonce: u64 },
}

async fn inventory_response(
    State(broker): State<InventoryDeadlineBroker>,
    axum::extract::Path(instance): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use std::sync::atomic::Ordering;
    assert_eq!(
        headers[axum::http::header::AUTHORIZATION],
        "Bearer local-startup-broker"
    );
    broker.registrations.fetch_add(1, Ordering::SeqCst);
    if broker.healthy.load(Ordering::SeqCst) {
        axum::Json(json!({
            "instance_id": instance, "change": "updated", "created_at": 0, "updated_at": 0
        }))
        .into_response()
    } else {
        (
            StatusCode::GATEWAY_TIMEOUT,
            axum::Json(json!({"error":{"code":"deadline"}})),
        )
            .into_response()
    }
}

async fn publisher_socket(
    State(broker): State<InventoryDeadlineBroker>,
    headers: axum::http::HeaderMap,
    upgrade: axum::extract::ws::WebSocketUpgrade,
) -> axum::response::Response {
    use axum::extract::ws::{CloseFrame, Message};
    use std::sync::atomic::Ordering;
    assert_eq!(
        headers[axum::http::header::AUTHORIZATION],
        "Bearer local-startup-broker"
    );
    upgrade.on_upgrade(move |mut socket| async move {
        let Some(Ok(Message::Text(initial))) = socket.recv().await else { return };
        let publish: serde_json::Value = serde_json::from_str(&initial).unwrap();
        assert_eq!(publish["op"], "publish");
        let target = publish["project_id"] == broker.target;
        let connections = if target { &broker.target_connections } else { &broker.sibling_connections };
        let connection = connections.fetch_add(1, Ordering::SeqCst) + 1;
        if socket.send(Message::Text(
            json!({"type":"welcome","heartbeat_timeout_ns":"4000000000"}).to_string().into()
        )).await.is_err() { return; }
        // A Pong after Welcome is observed only after the real publisher has
        // completed startup and entered its next() loop.
        if socket.send(Message::Ping(b"ready".to_vec().into())).await.is_err() { return; }
        let mut disconnect = broker.disconnect.clone();
        let mut probe = broker.probe.clone();
        loop {
            tokio::select! {
                result = socket.recv() => match result {
                    Some(Ok(Message::Pong(bytes))) if bytes.as_ref() == b"ready" => {
                        let _ = broker.events.send(PublisherObservation::Ready { target, connection }).await;
                    },
                    Some(Ok(Message::Pong(bytes))) if bytes.len() == 8 => {
                        let nonce = u64::from_be_bytes(bytes.as_ref().try_into().unwrap());
                        let _ = broker.events.send(PublisherObservation::Pong { target, nonce }).await;
                    },
                    Some(Ok(Message::Ping(bytes))) => {
                        if socket.send(Message::Pong(bytes)).await.is_err() { break; }
                    },
                    Some(Ok(Message::Text(_))) => (),
                    _ => break,
                },
                changed = disconnect.changed(), if target && connection == 1 => {
                    if changed.is_err() { break; }
                    let close = *disconnect.borrow_and_update();
                    if close {
                        let _ = socket.send(Message::Close(Some(CloseFrame {
                            code: 1001, reason: "fixture disconnect".into(),
                        }))).await;
                        break;
                    }
                },
                changed = probe.changed() => {
                    if changed.is_err() { break; }
                    let nonce = *probe.borrow_and_update();
                    if socket.send(Message::Ping(nonce.to_be_bytes().to_vec().into())).await.is_err() { break; }
                },
            }
        }
    })
}

async fn next_observation(
    events: &mut tokio::sync::mpsc::Receiver<PublisherObservation>,
    expected: impl Fn(&PublisherObservation) -> bool,
) -> Result {
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = events.recv().await {
            if expected(&event) {
                return Ok(());
            }
        }
        Err(io::Error::other("Broker observation ended"))
    })
    .await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inventory_deadline_after_ready_isolates_publisher_and_explicit_admission_recovers()
-> Result {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let (_directory, owner, raw) = fixture_projects(&endpoint, 2)?;
    let selected = Arc::clone(owner.projects.as_ref().unwrap());
    let installation = owner.project_control.as_ref().unwrap().installation.clone();
    let mut terminal = owner.now_terminal_effect.subscribe();
    let (disconnect, wait_disconnect) = watch::channel(false);
    let (probe, wait_probe) = watch::channel(0);
    let (events, mut observations) = tokio::sync::mpsc::channel(16);
    let broker = InventoryDeadlineBroker {
        target: selected[1].1.to_string(),
        healthy: Arc::new(AtomicBool::new(false)),
        registrations: Arc::new(AtomicUsize::new(0)),
        target_connections: Arc::new(AtomicUsize::new(0)),
        sibling_connections: Arc::new(AtomicUsize::new(0)),
        disconnect: wait_disconnect,
        probe: wait_probe,
        events,
    };
    let remote = Router::new()
        .route("/v1/realtime", axum::routing::get(publisher_socket))
        .route(
            "/v1/instances/{instance}/inventory",
            axum::routing::put(inventory_response),
        )
        .with_state(broker.clone());
    let remote_stop = CancellationToken::new();
    let remote_cancel = remote_stop.clone();
    let mut remote_job = tokio::task::JoinSet::new();
    remote_job.spawn(async move {
        axum::serve(listener, remote)
            .with_graceful_shutdown(remote_cancel.cancelled_owned())
            .await
    });
    let address = owner.address().unwrap();
    let service = Cancellation::default();
    let stop = service.clone();
    let mut running = tokio::task::JoinSet::new();
    running.spawn(async move { owner.run(&stop).await });
    let outcome: Result = async {
        let mut ready = [false; 2];
        tokio::time::timeout(Duration::from_secs(10), async {
            while !ready.iter().all(|ready| *ready) {
                let event = observations.recv().await.ok_or_else(|| io::Error::other("no initial publishers"))?;
                if let PublisherObservation::Ready { target, connection: 1 } = event {
                    ready[usize::from(target)] = true;
                }
            }
            Ok::<_, io::Error>(())
        }).await??;
        disconnect.send_replace(true);
        let effect = *tokio::time::timeout(Duration::from_secs(10),
            terminal.wait_for(|effect| effect.is_some())).await??;
        assert_eq!(effect, Some(graphit_http::client::NetworkEffect::MayHaveApplied));
        assert_eq!(broker.registrations.load(Ordering::SeqCst), 1);
        assert_eq!(broker.target_connections.load(Ordering::SeqCst), 1);
        let client = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build()?;
        let response = client.post(format!("http://{address}/admin/daemon"))
            .header("x-graphit-auth-origin", "runtime").bearer_auth(raw.as_str())
            .json(&json!({"operation":"status"})).send().await?;
        assert_eq!(response.status(), StatusCode::OK);
        let _: serde_json::Value = response.json().await?;
        let policy = HubResourcePolicy::resolve(&ConfigSnapshot::new(installation.brand(), [])?)?;
        let cancellation = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancellation)?;
        let prepared = rmcp::model::CallToolRequestParams::new("graphit_task_query").with_arguments(
            json!({"ai_optimized":false,"selection":{"project_dir":selected[0].0.root().to_str().unwrap()},
                "input":{"selection":{"table":"tasks","status":null},"columns":["id"],"page_size":1,"top_k":1}})
                .as_object().unwrap().clone());
        let mut output = Vec::new();
        crate::task_entry::run(&installation, &client, prepared, policy.content(), &original, &mut output).await?;
        let result: serde_json::Value = serde_json::from_slice(&output)?;
        assert_eq!(result["operation"], "query");
        assert_eq!(result["result"]["state"], "no_corpus");
        probe.send_replace(1);
        next_observation(&mut observations, |event|
            matches!(event, PublisherObservation::Pong { target: false, nonce: 1 })).await?;
        assert!(running.try_join_next().is_none());
        // The failed job has retired; this is a new explicit admission after
        // the Broker recovers, not a retry of the uncertain inventory request.
        broker.healthy.store(true, Ordering::SeqCst);
        let command = crate::foreground_project_control::ProjectCommand::MaintainCheckout {
            project_id: selected[1].1,
            project_dir: graphit_core::native_path::NativePath::new(selected[1].0.root().to_owned())?,
            version: selected[1].2.version().to_owned(), detached_base: String::new(), select: true,
        };
        let response = client.post(format!("http://{address}/admin/projects"))
            .header("x-graphit-auth-origin", "runtime").bearer_auth(raw.as_str())
            .json(&command).send().await?;
        let status = response.status();
        let body: serde_json::Value = response.json().await?;
        assert_eq!(status, StatusCode::OK, "{body}");
        next_observation(&mut observations, |event|
            matches!(event, PublisherObservation::Ready { target: true, connection: 2 })).await?;
        assert_eq!(broker.registrations.load(Ordering::SeqCst), 2);
        assert_eq!(broker.sibling_connections.load(Ordering::SeqCst), 1);
        assert!(running.try_join_next().is_none());
        Ok(())
    }.await;
    service.cancel();
    let stopped = tokio::time::timeout(Duration::from_secs(10), running.join_next()).await;
    remote_stop.cancel();
    let remote_stopped =
        tokio::time::timeout(Duration::from_secs(10), remote_job.join_next()).await;
    outcome?;
    stopped?.unwrap()??;
    remote_stopped?.unwrap()??;
    Ok(())
}

#[test]
fn inventory_deadline_classification_preserves_other_terminal_causes() {
    use graphit_daemon::broker_inventory::{
        InventoryPublicationRemoteCode, OwnInventoryPublicationFailure,
    };
    use graphit_http::client::NetworkEffect;
    for (status, code) in [
        (403, InventoryPublicationRemoteCode::Forbidden),
        (401, InventoryPublicationRemoteCode::Unauthenticated),
        (503, InventoryPublicationRemoteCode::OutcomeUncertain),
    ] {
        assert_eq!(
            inventory_deadline_effect(&io::Error::other(OwnInventoryPublicationFailure::Remote {
                status,
                code,
                effect: NetworkEffect::MayHaveApplied
            })),
            None
        );
    }
    for kind in [io::ErrorKind::Interrupted, io::ErrorKind::TimedOut] {
        assert_eq!(inventory_deadline_effect(&io::Error::from(kind)), None);
    }
    assert_eq!(
        inventory_deadline_effect(&io::Error::other(
            "Remote { status: 504, code: Deadline, effect: MayHaveApplied }"
        )),
        None
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inventory_deadline_at_bootstrap_keeps_failure_and_cleanup_owner() -> Result {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let (_directory, owner, _raw) = fixture(&endpoint)?;
    let control = owner.project_control.as_ref().unwrap();
    let installation = control.installation.clone();
    let config = control.config.clone();
    let selected = Arc::clone(owner.projects.as_ref().unwrap());
    owner.shutdown().await?;
    let remote = Router::new().fallback(|| async {
        (
            StatusCode::GATEWAY_TIMEOUT,
            axum::Json(json!({"error":{"code":"deadline"}})),
        )
    });
    let remote_stop = CancellationToken::new();
    let remote_cancel = remote_stop.clone();
    let mut remote_job = tokio::task::JoinSet::new();
    remote_job.spawn(async move {
        axum::serve(listener, remote)
            .with_graceful_shutdown(remote_cancel.cancelled_owned())
            .await
    });
    let outcome: Result = async {
        let cancellation = Cancellation::default();
        let policy = HubResourcePolicy::resolve(&config)?;
        let original = policy.request_context(Instant::now(), &cancellation)?;
        let inputs = [ProjectBootstrapInput {
            project: &selected[0].0,
            explicit_version: selected[0].2.version(),
            pin: None,
            detached_base: "",
        }];
        let mut failure = match ForegroundSource::start_and_publish(
            &installation,
            &inputs,
            0,
            &config,
            NativeBinding::Codex,
            Vec::new(),
            &original,
        )
        .await
        {
            Ok(owner) => {
                owner.shutdown().await?;
                return Err(io::Error::other("inventory bootstrap deadline became success").into());
            }
            Err(failure) => failure,
        };
        assert_eq!(
            inventory_deadline_effect(&failure.error),
            Some(graphit_http::client::NetworkEffect::MayHaveApplied)
        );
        failure
            .owner
            .take()
            .expect("failed startup must retain its cleanup owner")
            .shutdown()
            .await?;
        Ok(())
    }
    .await;
    remote_stop.cancel();
    let remote_stopped =
        tokio::time::timeout(Duration::from_secs(10), remote_job.join_next()).await;
    outcome?;
    remote_stopped?.unwrap()??;
    Ok(())
}

#[test]
fn cleanup_failure_debug_keeps_the_prior_service_cause() {
    let failure = ForegroundFailure {
        error: io::Error::other("cleanup failure"),
        owner: None,
        service: Some(io::Error::other("prior service cause")),
    };
    let debug = format!("{failure:?}");
    assert!(debug.contains("cleanup failure"));
    assert!(debug.contains("prior service cause"));
}
