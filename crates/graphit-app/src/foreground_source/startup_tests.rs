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
    artifact_store::{CapturedPublicationScope, ScopeLimits},
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
    let private = installation_bootstrap::prepare(
        &installation,
        BootstrapLimits::new(65536, 256)?,
        original.snapshot(),
    )
    .into_installation()?;
    let identity = InstallationIdentityStore::new(&installation)?
        .ensure(Duration::from_secs(5), &cancellation)?;
    private.revalidate(original.snapshot())?;
    let scope = CapturedPublicationScope::installation(
        &installation,
        identity.unit_id().clone(),
        [installation.root().to_owned()],
        ScopeLimits::new(16, 65536, 256)?,
        original.snapshot(),
    )?;
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            scope,
            AuthDocumentLimits::new(1048576, 65536, 256, 65536)?,
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
    assert!(published.failure().is_none());
    assert!(published.cleanup_failure().is_none());
    drop(published);
    drop(store);
    let project_root = directory.path().join("project");
    std::fs::create_dir(&project_root)?;
    let project = ProjectPaths::at(&project_root, &installation)?;
    ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(5), &cancellation)?
        .ensure(&ProjectMetadata::new("startup")?)?;
    let provider = active_provider(&installation, &config, &original)?;
    let owner = ForegroundSource::start_with_inventory(
        &installation,
        &[ProjectBootstrapInput {
            project: &project,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        }],
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
                // Reuse the observed request's signal to mark the response boundary.
                observed.send_replace(false);
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
        tokio::time::timeout(
            Duration::from_secs(10),
            observation.wait_for(|pending| !*pending),
        )
        .await??;
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
#[test]
fn cleanup_failure_debug_keeps_the_prior_service_cause() {
    let failure = ForegroundFailure {
        error: io::Error::other("cleanup failure"),
        owner: None,
        runtime: None,
        service: Some(io::Error::other("prior service cause")),
    };
    let debug = format!("{failure:?}");
    assert!(debug.contains("cleanup failure"));
    assert!(debug.contains("prior service cause"));
}
