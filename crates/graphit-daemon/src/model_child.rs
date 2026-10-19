//! Dedicated compute child startup. The parent remains the runtime-key owner.
mod launcher;
pub use launcher::{ChildLaunch, ChildProcessResult, ChildShutdown, ChildStartError, ModelChild};

use crate::{model_service::*, model_transport::DisconnectTransport};
use graphit_ai::{
    context::InferenceContext, embedding::EmbeddingLimits, model_manifest::ManifestLimits,
    native_embedding::NativeSdkRoot, native_rerank::NativeRerankLimits,
};
use graphit_auth::{
    profile::{AiService, ProfileError, ProfileLimits},
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet,
        ScopeLimits, SnapshotContext, SnapshotPolicy, SourceReadLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    config::{ConfigSnapshot, ConfigStore},
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationIdentityStore,
    native_document_file::CapturedDocumentDirectory,
    native_path::MAX_PATH_BYTES,
    project_paths::InstallationPaths,
    tcp_listener::{self, PortSelection},
};
use graphit_hub::policy::{HubResourcePolicy, RequestContext, ResourceSetting};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
    sync::Semaphore,
    task::JoinSet,
};
use zeroize::Zeroizing;

/// Ordinary process defaults; library callers can supply their own finite options.
pub struct ChildOptions {
    pub startup_timeout: Duration,
    pub shutdown_timeout: Duration,
    pub listener: tcp_listener::Options,
    pub connections: NonZeroUsize,
    pub service: ServiceLimits,
    pub rpc: RpcLimits,
    pub manifest: ManifestLimits,
    pub capture: SnapshotPolicy,
    pub source: SourceReadLimits,
    pub bootstrap: BootstrapLimits,
}
impl ChildOptions {
    pub fn standard() -> io::Result<Self> {
        let retention = RetainedBytesLimits::new(1, 1, 4096)?;
        Ok(Self {
            startup_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(30),
            listener: tcp_listener::Options {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                ports: PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            connections: NonZeroUsize::new(4).ok_or(io::ErrorKind::InvalidInput)?,
            service: ServiceLimits {
                max_active_workers: NonZeroUsize::new(1).ok_or(io::ErrorKind::InvalidInput)?,
                embedding: EmbeddingLimits::new(
                    2048,
                    8 * 1024 * 1024,
                    65536,
                    4 * 1024 * 1024,
                    128,
                    16 * 1024 * 1024,
                    64 * 1024 * 1024,
                )
                .map_err(io::Error::other)?,
                rerank: NativeRerankLimits::new(
                    2048,
                    128,
                    8 * 1024 * 1024,
                    16 * 1024 * 1024,
                    64 * 1024 * 1024,
                )
                .map_err(io::Error::other)?,
            },
            rpc: RpcLimits::new(32 * 1024 * 1024, 2048, 8 * 1024 * 1024, 4 * 1024 * 1024)
                .map_err(io::Error::other)?,
            manifest: ManifestLimits {
                max_manifest_bytes: 1024 * 1024,
                max_artifacts: 128,
                max_role_bytes: 256,
                max_relative_path_bytes: 4096,
                max_id_bytes: 4096,
                max_revision_bytes: 4096,
                max_dimension: 65536,
                max_tokens: 1024 * 1024,
            },
            capture: SnapshotPolicy::new(
                CopyPolicy::RejectFileLinks,
                CopyLimits::new(64 * 1024 * 1024 * 1024, 1024, 65536, 64)?,
                RetainedFileSet::new([], retention)?,
                retention,
            )?,
            source: SourceReadLimits::new(1024 * 1024, 4096, 1024 * 1024)?,
            bootstrap: BootstrapLimits::new(65536, 256).map_err(io::Error::other)?,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Readiness {
    pub address: SocketAddr,
}

fn deadline(timeout: Duration) -> io::Result<Instant> {
    if timeout.is_zero() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Instant::now()
        .checked_add(timeout)
        .ok_or(io::ErrorKind::InvalidInput.into())
}

async fn read_secret<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<RuntimeSecret> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(65));
    reader.take(65).read_to_end(&mut bytes).await?;
    if bytes.len() != 64 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut key = Zeroizing::new([0u8; 64]);
    key.copy_from_slice(&bytes);
    RuntimeSecret::new(key).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Global configuration source captured by the child, independent of any project.
pub struct GlobalModelConfig {
    root: PathBuf,
    capture: SnapshotPolicy,
    source: SourceReadLimits,
    manifest: ManifestLimits,
    refresh: Arc<tokio::sync::Mutex<()>>,
    pending: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}
impl GlobalModelConfig {
    pub fn new(
        root: PathBuf,
        capture: SnapshotPolicy,
        source: SourceReadLimits,
        manifest: ManifestLimits,
    ) -> ModelResult<Self> {
        if !root.is_absolute() {
            return Err(ModelServiceError::new(ModelErrorKind::Configuration));
        }
        Ok(Self {
            root,
            capture,
            source,
            manifest,
            refresh: Arc::new(tokio::sync::Mutex::new(())),
            pending: Mutex::new(Vec::new()),
        })
    }

    pub(crate) async fn refresh(
        self: &Arc<Self>,
        service: ModelService,
        expected: BTreeMap<String, ConfigId>,
        original: Arc<SnapshotContext>,
    ) -> ModelResult<RefreshOutput> {
        let serial = self.serialize(&original).await?;
        let deadline = original
            .required_deadline()
            .map_err(ModelServiceError::io)?;
        let source = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        let join = tokio::task::spawn_blocking(move || {
            // Serialization outlives a dropped RPC future until the actual capture terminates.
            let _serial = serial;
            let result = (|| {
                let slots = expected.keys().cloned().collect::<Vec<_>>();
                let config = source
                    .resolve(original.clone(), &slots)?
                    .unwrap_or_else(ResolvedConfig::empty);
                let config_ids = config.config_ids();
                // The owner supplies its current demands, not provider availability.
                // Identities are checked against fresh configuration before install.
                if expected
                    .iter()
                    .any(|(slot, id)| config_ids.get(slot) != Some(id))
                {
                    return Err(ModelServiceError::new(ModelErrorKind::ConfigChanged));
                }
                let generation = service.install(
                    config,
                    InferenceContext::new(original.cancellation(), deadline),
                )?;
                Ok(RefreshOutput {
                    config_ids,
                    generation,
                })
            })();
            let _ = send.send(result);
        });
        {
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            pending.push(join);
        }
        receive
            .await
            .map_err(|error| ModelServiceError::caused(ModelErrorKind::Worker, error))?
    }

    async fn serialize(
        &self,
        original: &SnapshotContext,
    ) -> ModelResult<tokio::sync::OwnedMutexGuard<()>> {
        let completed = {
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            let (completed, active): (Vec<_>, Vec<_>) = std::mem::take(&mut *pending)
                .into_iter()
                .partition(tokio::task::JoinHandle::is_finished);
            *pending = active;
            completed
        };
        for join in completed {
            join.await
                .map_err(|error| ModelServiceError::caused(ModelErrorKind::Worker, error))?;
        }
        let deadline = original
            .required_deadline()
            .map_err(ModelServiceError::io)?;
        let serial = tokio::select! {
            biased;
            _ = original.cancellation().cancelled() => return Err(ModelServiceError::new(ModelErrorKind::Cancelled)),
            _ = tokio::time::sleep_until(deadline.into()) => return Err(ModelServiceError::new(ModelErrorKind::Deadline)),
            guard = self.refresh.clone().lock_owned() => guard,
        };
        original.check().map_err(ModelServiceError::io)?;
        Ok(serial)
    }

    /// Admit an absent slot for the RPC's actual purpose, preserving existing demands.
    pub(crate) async fn ensure_model(
        self: &Arc<Self>,
        service: ModelService,
        slot: String,
        expected: ConfigId,
        task: graphit_ai::model_manifest::ModelTask,
        original: Arc<SnapshotContext>,
    ) -> ModelResult<()> {
        let serial = self.serialize(&original).await?;
        let deadline = original
            .required_deadline()
            .map_err(ModelServiceError::io)?;
        let source = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        let join = tokio::task::spawn_blocking(move || {
            let _serial = serial;
            let result = (|| {
                let context = InferenceContext::new(original.cancellation(), deadline);
                if service.has_model(&slot, expected, task, context)? {
                    return Ok(());
                }
                let mut slots = service.slot_names(context)?;
                slots.push(slot.clone());
                let config = source
                    .resolve(original.clone(), &slots)?
                    .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Selection))?;
                let selected = config
                    .slots
                    .get(&slot)
                    .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Selection))?;
                if selected.manifest.task != task {
                    return Err(ModelServiceError::new(ModelErrorKind::Selection));
                }
                if selected.id != expected {
                    return Err(ModelServiceError::new(ModelErrorKind::ConfigChanged));
                }
                service.install(config, context)?;
                Ok(())
            })();
            let _ = send.send(result);
        });
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(join);
        receive
            .await
            .map_err(|error| ModelServiceError::caused(ModelErrorKind::Worker, error))?
    }

    pub(crate) async fn join(&self) -> ModelResult<()> {
        let pending = std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()));
        let mut result = Ok(());
        for join in pending {
            if let Err(error) = join.await {
                result = Err(ModelServiceError::caused(ModelErrorKind::Worker, error));
            }
        }
        result
    }

    pub(crate) fn resolve(
        &self,
        original: Arc<SnapshotContext>,
        requested_slots: &[String],
    ) -> ModelResult<Option<ResolvedConfig>> {
        original.check().map_err(ModelServiceError::io)?;
        let result = (|| {
            let brand = Brand::default();
            let paths =
                InstallationPaths::resolve(brand.clone(), &self.root, None, Some(&self.root))
                    .map_err(ModelServiceError::io)?;
            let store = ConfigStore::global(&paths).map_err(ModelServiceError::io)?;
            let layer = store.load().map_err(ModelServiceError::io)?;
            let snapshot = ConfigSnapshot::new(&brand, [layer]).map_err(ModelServiceError::io)?;
            let policy =
                HubResourcePolicy::resolve(&snapshot).map_err(ModelServiceError::configuration)?;
            // Auth parsing and model capture share this finite subcontext. It cannot extend
            // or replace the caller's absolute deadline/cancellation; the outer request is rechecked.
            let context = RequestContext::new(
                policy.content(),
                original.cancellation(),
                original
                    .required_deadline()
                    .map_err(ModelServiceError::io)?,
            )
            .map_err(ModelServiceError::configuration)?;
            let auth = capture_auth(&paths, &policy, &context)?;
            let provider = if auth.was_present() {
                match auth.document().active(&context) {
                    Ok((provider, _)) => provider,
                    Err(ProfileError::NoActive) => return Ok(None),
                    Err(error) => {
                        return Err(ModelServiceError::caused(
                            ModelErrorKind::Authentication,
                            error,
                        ));
                    }
                }
            } else {
                return Ok(None);
            };
            original.check().map_err(ModelServiceError::io)?;
            if !matches!(provider.config().embedding, AiService::Local(_))
                && !matches!(provider.config().rerank, AiService::Local(_))
            {
                return Ok(None);
            }
            let idle_timeout = ModelSettings::idle_timeout(&snapshot)?;
            if requested_slots.is_empty() {
                context.check().map_err(ModelServiceError::io)?;
                return Ok(Some(
                    ResolvedConfig::empty().with_idle_timeout(idle_timeout)?,
                ));
            }
            let Some(mut settings) = ModelSettings::read(&snapshot)? else {
                original.check().map_err(ModelServiceError::io)?;
                return Ok(None);
            };
            settings
                .slots
                .retain(|slot| requested_slots.contains(&slot.name));
            let embedding = settings
                .slots
                .iter()
                .any(|slot| slot.selection.task == TaskKind::Embedding);
            let rerank = settings
                .slots
                .iter()
                .any(|slot| slot.selection.task == TaskKind::Rerank);
            let selection = LocalExecutionSelection::from_services(
                if embedding {
                    &provider.config().embedding
                } else {
                    &AiService::Disabled
                },
                &provider.config().rerank,
                rerank,
            )?;
            settings
                .slots
                .retain(|slot| selection.for_task(slot.selection.task).is_some());
            for slot in &mut settings.slots {
                if slot.bundle.is_relative() {
                    slot.bundle = self.root.join(&slot.bundle);
                }
            }
            let resolved = ResolvedConfig::resolve_declared(
                settings,
                selection,
                context.snapshot(),
                &self.capture,
                self.source,
                &self.manifest,
            )?
            .with_idle_timeout(idle_timeout)?;
            original.check().map_err(ModelServiceError::io)?;
            context.check().map_err(ModelServiceError::io)?;
            Ok(Some(resolved))
        })();
        original.check().map_err(ModelServiceError::io)?;
        result
    }
}

fn capture_auth(
    paths: &InstallationPaths,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> ModelResult<graphit_auth::store::AuthSnapshot> {
    let number = |setting| {
        usize::try_from(policy.limit(setting).value).map_err(ModelServiceError::configuration)
    };
    let depth = number(ResourceSetting::SnapshotDepth)?;
    let bytes = number(ResourceSetting::ContentInputBytes)?;
    let scalar = number(ResourceSetting::ContentScalarBytes)?;
    let entries = number(ResourceSetting::ContentCollectionEntries)?;
    let bootstrap =
        BootstrapLimits::new(MAX_PATH_BYTES, depth).map_err(ModelServiceError::configuration)?;
    let private = installation_bootstrap::prepare(paths, bootstrap, original.snapshot())
        .into_installation()
        .map_err(ModelServiceError::configuration)?;
    original.check().map_err(ModelServiceError::io)?;
    let remaining = original
        .snapshot()
        .required_deadline()
        .map_err(ModelServiceError::io)?
        .checked_duration_since(Instant::now())
        .ok_or_else(|| ModelServiceError::new(ModelErrorKind::Deadline))?;
    let identity = InstallationIdentityStore::new(paths)
        .map_err(ModelServiceError::io)?
        .ensure(remaining, original.cancellation())
        .map_err(ModelServiceError::io)?;
    private
        .revalidate(original.snapshot())
        .map_err(ModelServiceError::configuration)?;
    let scope = CapturedPublicationScope::installation(
        paths,
        identity.unit_id().clone(),
        [paths.root().to_owned()],
        ScopeLimits::new(1, MAX_PATH_BYTES, depth).map_err(ModelServiceError::io)?,
        original.snapshot(),
    )
    .map_err(ModelServiceError::io)?;
    let physical = AuthDocumentStore::capture(
        scope,
        AuthDocumentLimits::new(bytes, MAX_PATH_BYTES, depth, scalar)
            .map_err(ModelServiceError::configuration)?,
        original.snapshot(),
    )
    .map_err(ModelServiceError::configuration)?;
    let store = AuthStore::new(
        physical,
        ProfileLimits {
            bytes,
            nodes: number(ResourceSetting::ContentNodes)?,
            scalar_bytes: scalar,
            collection_entries: entries,
            providers: entries,
            profiles: entries,
        },
        original,
    )
    .map_err(ModelServiceError::configuration)?;
    store
        .load(original)
        .map_err(|e| ModelServiceError::caused(ModelErrorKind::Authentication, e))
}

/// Startup contexts are released; the global source and native bootstrap live until joins finish.
pub async fn serve_child<R, W>(
    root: &Path,
    options: ChildOptions,
    mut bootstrap: R,
    mut ready: W,
    lifetime: Cancellation,
) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    if !root.is_absolute()
        || !options.listener.address.is_loopback()
        || options.connections.get() > Semaphore::MAX_PERMITS
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    deadline(options.shutdown_timeout)?;
    let original = Arc::new(SnapshotContext::new(
        deadline(options.startup_timeout)?,
        lifetime.clone(),
    ));
    let startup_deadline = original.required_deadline()?;
    let secret = tokio::select! {
        biased;
        _ = lifetime.cancelled() => return Err(io::ErrorKind::Interrupted.into()),
        _ = tokio::time::sleep_until(startup_deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
        secret = read_secret(&mut bootstrap) => Arc::new(secret?),
    };
    original.check()?;
    let brand = Brand::default();
    let paths = InstallationPaths::resolve(brand.clone(), root, None, Some(root))?;
    let private = installation_bootstrap::prepare(&paths, options.bootstrap, &original)
        .into_installation()
        .map_err(io::Error::other)?;
    let global = Arc::new(
        GlobalModelConfig::new(
            root.to_owned(),
            options.capture,
            options.source,
            options.manifest,
        )
        .map_err(io::Error::other)?,
    );
    let config = global
        .resolve(original.clone(), &[])
        .map_err(io::Error::other)?
        .ok_or(io::ErrorKind::NotFound)?;
    // The SDK bootstrap owns no model. Admitted RPCs populate the cold catalog.
    let _sdk_root = NativeSdkRoot::new()
        .map_err(ModelServiceError::from)
        .map_err(io::Error::other)?;
    // Local infrastructure, independent of model identity and configured bundle paths.
    let snapshot_path = paths.root().join("tmp");
    private.revalidate(&original).map_err(io::Error::other)?;
    let snapshot_birth = match graphit_fs::create_private_directory(&snapshot_path) {
        Ok(handle) => Some(handle),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(&snapshot_path)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            None
        }
        Err(error) => return Err(error),
    };
    let expected_snapshot_path = std::fs::canonicalize(paths.root())?.join("tmp");
    let snapshot_parent = Arc::new(CapturedDocumentDirectory::capture(
        &snapshot_path,
        options.bootstrap.native_limits(),
        &original,
    )?);
    let metadata = std::fs::symlink_metadata(&snapshot_path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || snapshot_parent.canonical_path() != expected_snapshot_path
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if let Some(handle) = &snapshot_birth {
        snapshot_parent.verify_private_handle(handle, &original)?;
    } else {
        snapshot_parent.private_handle(&original)?;
    }
    private.revalidate(&original).map_err(io::Error::other)?;
    let service = ModelService::new(snapshot_parent, options.service)
        .and_then(|service| service.with_shutdown_timeout(options.shutdown_timeout))
        .map_err(io::Error::other)?;
    service
        .install(config, InferenceContext::new(&lifetime, startup_deadline))
        .map_err(io::Error::other)?;
    let bound = tcp_listener::reserve(options.listener, &lifetime).map_err(io::Error::other)?;
    original.check()?;
    let address = bound.address();
    let listener = bound.into_listener();
    listener.set_nonblocking(true)?;
    let listener = TcpListener::from_std(listener)?;
    let mut message = serde_json::to_vec(&Readiness { address })?;
    message.push(b'\n');
    tokio::select! {
        biased;
        _ = lifetime.cancelled() => return Err(io::ErrorKind::Interrupted.into()),
        _ = tokio::time::sleep_until(startup_deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
        result = async { ready.write_all(&message).await?; ready.flush().await } => result?,
    }
    drop(original);
    drop(private);
    drop(bootstrap);
    drop(ready);
    let permits = Arc::new(Semaphore::new(options.connections.get()));
    let mut connections = JoinSet::new();
    let result = loop {
        let permit = tokio::select! {
            biased;
            _ = lifetime.cancelled() => break Ok(()),
            finished = connections.join_next(), if !connections.is_empty() => {
                if let Some(Err(error)) = finished { break Err(io::Error::other(error)); }
                continue;
            },
            permit = permits.clone().acquire_owned() => match permit {
                Ok(permit) => permit,
                Err(error) => break Err(io::Error::other(error)),
            },
        };
        let stream = tokio::select! {
            biased;
            _ = lifetime.cancelled() => break Ok(()),
            stream = listener.accept() => match stream { Ok((stream, _)) => stream,
                Err(error) => break Err(error) },
        };
        let connection = Cancellation::default();
        let transport = match server_transport(stream, options.rpc) {
            Ok(transport) => DisconnectTransport::new(transport, connection.clone()),
            Err(error) => break Err(io::Error::other(error)),
        };
        let service = service.clone();
        let secret = secret.clone();
        let limits = options.rpc;
        let global = global.clone();
        connections.spawn(async move {
            let _permit = permit;
            serve_connection(transport, service, secret, limits, global, connection).await
        });
    };
    drop(listener);
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    let refreshed = global.join().await.map_err(io::Error::other);
    let cleanup = Cancellation::default();
    let shutdown = match deadline(options.shutdown_timeout) {
        Ok(deadline) => service
            .shutdown(InferenceContext::new(&cleanup, deadline))
            .await
            .map_err(io::Error::other),
        Err(error) => Err(error),
    };
    let joined = service.join_shutdown().await.map_err(io::Error::other);
    result.and(refreshed).and(shutdown).and(joined)
}

#[cfg(test)]
mod lazy_admission_tests {
    use super::*;

    #[tokio::test]
    async fn waiting_refresh_lock_preserves_absolute_deadline_and_cancellation() {
        let root = tempfile::tempdir().unwrap();
        let options = ChildOptions::standard().unwrap();
        let source = GlobalModelConfig::new(
            root.path().to_owned(),
            options.capture,
            options.source,
            options.manifest,
        )
        .unwrap();
        let held = source.refresh.lock().await;
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_millis(5),
            Cancellation::default(),
        );
        assert_eq!(
            source.serialize(&original).await.unwrap_err().kind(),
            ModelErrorKind::Deadline
        );
        let cancel = Cancellation::default();
        cancel.cancel();
        let original = SnapshotContext::new(Instant::now() + Duration::from_secs(1), cancel);
        assert_eq!(
            source.serialize(&original).await.unwrap_err().kind(),
            ModelErrorKind::Cancelled
        );
        drop(held);
        source.join().await.unwrap();
    }
}
