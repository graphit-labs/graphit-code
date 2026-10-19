//! USER test writes use a real signed incoming MCP proof and native admission.
use super::*;
use graphit_auth::{
    UserMemoryWriteOperation,
    broker::BrokerRequestContext,
    profile::{AiService, BrokerService, ProviderConfig, ProviderKind, S3Location, S3Source},
    profile_manager::{NativeProfileManager, ProfileManagerAuthority},
};
use graphit_core::{artifact_store::UserIdentity, project_paths::InstallationPaths};
use graphit_records::{
    memory_store::{Memory, MemoryStoreWriteLimits},
    user_memory_store::NativeUserMemoryStore,
};
pub(crate) const RESOURCE: &str = "https://mcp.example/user-memory";
pub(crate) use super::memory_fixture::Seed;

pub(crate) fn provider(server: &TaskServer, runtime: &str, endpoint: &str) -> u64 {
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(runtime, &original).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let manager = NativeProfileManager::new(issuer);
    let snapshot = manager
        .begin(ProfileManagerAuthority::Admin(&parent))
        .unwrap();
    let revision = snapshot.revision();
    let publication = manager
        .add_provider(
            snapshot,
            "seed".into(),
            ProviderConfig {
                kind: ProviderKind::Broker(BrokerService {
                    endpoint: endpoint.into(),
                    mcp_resource: Some(RESOURCE.into()),
                    allow_anonymous: false,
                }),
                embedding: AiService::Disabled,
                rerank: AiService::Disabled,
                s3: S3Location {
                    bucket: None,
                    region: None,
                    endpoint: None,
                    prefix: None,
                    source: S3Source::BrokerScope,
                },
            },
            revision,
            1,
        )
        .unwrap();
    assert!(publication.failure().is_none() && publication.cleanup_failure().is_none());
    assert!(publication.into_pending_cleanup().is_none());
    let snapshot = manager
        .begin(ProfileManagerAuthority::Admin(&parent))
        .unwrap();
    snapshot
        .providers()
        .find(|p| p.name() == "seed")
        .unwrap()
        .revision()
}

pub(crate) async fn seed(
    server: &TaskServer,
    verifier: &BrokerVerifier,
    token: &str,
    operation: Seed,
) -> Memory {
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let proof = verifier
        .verify(token, BrokerRequestContext::new(original.snapshot()))
        .await
        .unwrap();
    let issuer = server.0.auth.issuer();
    let verified = issuer
        .user_from_incoming(&proof, verifier, &original)
        .unwrap();
    let installation = server.0.auth.installation().live;
    let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
        installation.owner()
    else {
        panic!("installation owner")
    };
    let paths = InstallationPaths::resolve(
        installation.brand().clone(),
        installation.owner_root(),
        None,
        Some(installation.owner_root()),
    )
    .unwrap();
    let user = CapturedPublicationScope::user(
        &paths,
        installation_id.clone(),
        UserIdentity::new(
            verified.issuer().unwrap().into(),
            verified.subject().unwrap().into(),
        )
        .unwrap(),
        server.0.policy.scope,
        original.snapshot(),
    )
    .unwrap();
    let binding = user.binding().unwrap();
    let admission = issuer
        .admit_incoming_user_memory_write(
            &proof,
            verifier,
            &original,
            "seed",
            ScopeCapture {
                live: &user,
                record: &binding,
            },
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    let limits = MemoryStoreWriteLimits {
        read: crate::foreground_memory::limits(&server.0.policy).unwrap(),
        artifacts: server.0.policy.write.artifacts,
        max_input_bytes: std::num::NonZeroUsize::new(server.0.policy.content.max_input_bytes)
            .unwrap(),
        max_row_bytes: std::num::NonZeroUsize::new(server.0.policy.content.max_output_bytes)
            .unwrap(),
    };
    match operation {
        Seed::Create(input) => {
            NativeUserMemoryStore::create(original.snapshot(), &admission, input, limits)
                .await
                .unwrap()
        }
        Seed::Update(id, input) => {
            NativeUserMemoryStore::update(original.snapshot(), &admission, id, input, limits)
                .await
                .unwrap()
        }
        Seed::Delete(id, input) => {
            NativeUserMemoryStore::delete(original.snapshot(), &admission, id, input, limits)
                .await
                .unwrap()
        }
    }
}

/// Seeds embeddings using genuine USER admissions and the already running CPU model.
pub(crate) async fn index_for_search(
    server: &TaskServer,
    verifier: &BrokerVerifier,
    token: &str,
    resources: &crate::foreground_memory_jobs::MemoryJobResources,
) {
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let proof = verifier
        .verify(token, BrokerRequestContext::new(original.snapshot()))
        .await
        .unwrap();
    let issuer = server.0.auth.issuer();
    let verified = issuer
        .user_from_incoming(&proof, verifier, &original)
        .unwrap();
    let installation = server.0.auth.installation().live;
    let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
        installation.owner()
    else {
        panic!("installation owner")
    };
    let paths = InstallationPaths::resolve(
        installation.brand().clone(),
        installation.owner_root(),
        None,
        Some(installation.owner_root()),
    )
    .unwrap();
    let user = CapturedPublicationScope::user(
        &paths,
        installation_id.clone(),
        UserIdentity::new(
            verified.issuer().unwrap().into(),
            verified.subject().unwrap().into(),
        )
        .unwrap(),
        server.0.policy.scope,
        original.snapshot(),
    )
    .unwrap();
    let binding = user.binding().unwrap();
    let admission = issuer
        .admit_incoming_user_memory_write(
            &proof,
            verifier,
            &original,
            "seed",
            ScopeCapture {
                live: &user,
                record: &binding,
            },
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    let limits = MemoryStoreWriteLimits {
        read: crate::foreground_memory::limits(&server.0.policy).unwrap(),
        artifacts: server.0.policy.write.artifacts,
        max_input_bytes: std::num::NonZeroUsize::new(server.0.policy.content.max_input_bytes)
            .unwrap(),
        max_row_bytes: std::num::NonZeroUsize::new(server.0.policy.content.max_output_bytes)
            .unwrap(),
    };

    let read = issuer
        .admit_incoming_user_memory_read(
            &proof,
            verifier,
            &original,
            "seed",
            ScopeCapture {
                live: &user,
                record: &binding,
            },
        )
        .unwrap();
    let model_key = resources.auth.runtime().check(original.snapshot()).unwrap();
    let model_raw = std::str::from_utf8(model_key.secret(original.snapshot()).unwrap()).unwrap();
    let model_caller = resources
        .auth
        .issuer()
        .authenticate_runtime(model_raw, &original)
        .unwrap();
    let mut service = resources
        .auth
        .issuer()
        .capture_service(&model_caller, graphit_auth::ServicePurpose::Embedding)
        .unwrap();
    let lease = service.local().unwrap();
    let local = resources
        .local
        .as_ref()
        .expect("already running local USER memory model");
    let identity = graphit_records::memory_store::MemoryEmbeddingIdentity::new(
        local.expected.vector_id(),
        std::num::NonZeroU32::new(u32::try_from(local.expected.dimensions().unwrap()).unwrap())
            .unwrap(),
        graphit_records::memory_store::MEMORY_DOCUMENT_RECIPE.into(),
    )
    .unwrap();
    let graphit_records::user_memory_store::UserMemoryEmbeddingCorpusRead::Published(mut corpus) =
        NativeUserMemoryStore::capture_embeddings(
            original.snapshot(),
            &read,
            identity,
            limits.read,
        )
        .await
        .unwrap()
    else {
        panic!("genuine seeded USER corpus")
    };
    while let Some(documents) = corpus
        .next_documents(original.snapshot(), &read)
        .await
        .unwrap()
    {
        let (mut attachments, texts): (Vec<_>, Vec<_>) = documents
            .into_iter()
            .map(|document| {
                let attachment = graphit_records::memory_store::MemoryEmbeddingAttachment {
                    memory_id: document.memory_id(),
                    revision: std::num::NonZeroU64::new(document.revision()).unwrap(),
                    input_fingerprint: *document.fingerprint(),
                    vector: Vec::new(),
                };
                (attachment, document.into_text())
            })
            .unzip();
        let vectors = local
            .client
            .embed(
                &lease,
                resources.auth.runtime(),
                &local.expected,
                graphit_daemon::model_service::EmbedOperation {
                    input: graphit_daemon::model_service::EmbedInput::Document,
                    texts,
                    deadline: original.snapshot().required_deadline().unwrap(),
                },
                tokio::net::TcpStream::connect(local.address),
            )
            .await
            .unwrap();
        assert_eq!(vectors.vectors.len(), attachments.len());
        for (attachment, vector) in attachments.iter_mut().zip(vectors.vectors) {
            attachment.vector = vector;
        }
        NativeUserMemoryStore::attach_embeddings(
            original.snapshot(),
            &admission,
            &corpus,
            attachments,
            limits,
        )
        .await
        .unwrap();
    }
}

pub(crate) fn with_limits(server: &TaskServer, output: usize) -> TaskServer {
    let snapshot = server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let mut policy = server.0.policy.clone();
    policy.content.max_output_bytes = output;
    TaskServer::new(
        Arc::clone(&server.0.auth),
        Arc::clone(&snapshot.projects),
        snapshot.current.expect("active fixture"),
        policy,
        CancellationToken::new(),
    )
    .unwrap()
}

pub(crate) fn stop(server: &TaskServer) {
    server.0.stop.cancel();
}

pub(crate) fn user_runtime(server: &TaskServer, issuer: &str, subject: &str) -> std::path::PathBuf {
    let identity = UserIdentity::new(issuer.into(), subject.into()).unwrap();
    server
        .0
        .auth
        .installation()
        .live
        .owner_root()
        .join("users")
        .join(identity.namespace_key())
        .join("runtime")
}
pub(crate) fn corrupt(server: &TaskServer, issuer: &str, subject: &str) {
    let versions = user_runtime(server, issuer, subject).join("memory/index.lance/_versions");
    let manifests = std::fs::read_dir(versions)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "manifest"))
        .collect::<Vec<_>>();
    assert!(!manifests.is_empty());
    for manifest in manifests {
        std::fs::write(manifest, b"invalid actual SDK manifest").unwrap();
    }
}
