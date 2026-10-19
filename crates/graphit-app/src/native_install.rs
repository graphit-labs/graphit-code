//! Authenticated installation of native corpora into the installation's global cache.
use graphit_auth::{
    AuthError, AuthenticatedCaller, ExternalEffect, Permission, ServiceUseError,
    broker::{BrokerPolicy, BrokerVerifier, JsonLimits, VerifierLimits},
};
use graphit_core::{
    artifact_store::{CapturedSourceReader, CopyPolicy, SourceReadLimits, StoreOwner},
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
    project_identity::ProjectId,
    project_paths::InstallationPaths,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_data::{CapturedLocalGraph, LocalGraphPolicy, lance_module_store::LanceModuleStore};
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, ProjectionError, SourceRef,
    native_package::{self, PackageError, PackageKind},
    policy::{HubResourcePolicy, RequestContext, ResourceSetting},
    registry::{
        HubRegistry, HubScope, Installation, InstalledArtifact, InstalledBackend, InstalledPayload,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallRequest {
    pub input: PathBuf,
    pub producer: ProjectId,
    pub id: String,
    pub version: String,
    #[serde(
        default,
        deserialize_with = "present_kind",
        skip_serializing_if = "Option::is_none"
    )]
    pub kind: Option<PackageKind>,
}
fn present_kind<'de, D: serde::Deserializer<'de>>(
    value: D,
) -> Result<Option<PackageKind>, D::Error> {
    PackageKind::deserialize(value).map(Some)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum PayloadEffect {
    NotPublished,
    Published,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum RegistryEffect {
    NotAttempted,
    Registered,
    NotRegistered,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum FailureCategory {
    InvalidInput,
    Authentication,
    PermissionDenied,
    Package,
    Graph,
    Lance,
    AlreadyExists,
    Registry,
    Cancelled,
    Deadline,
    Delivery,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum InstallPhase {
    Input,
    Authentication,
    Capture,
    Stage,
    Admission,
    Publish,
    Registry,
    Delivery,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstallFailure {
    pub category: FailureCategory,
    pub phase: InstallPhase,
    /// Fixed public diagnostic. SDK causes are retained separately, never serialized.
    pub message: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstallOutcome {
    pub producer: ProjectId,
    pub id: String,
    pub version: String,
    pub kind: Option<PackageKind>,
    pub destination: Option<PathBuf>,
    pub payload: PayloadEffect,
    pub registry: RegistryEffect,
    pub failure: Option<InstallFailure>,
}
impl InstallOutcome {
    pub fn succeeded(&self) -> bool {
        self.payload == PayloadEffect::Published
            && self.registry == RegistryEffect::Registered
            && self.failure.is_none()
    }
}
/// Local causal evidence; callers send only `outcome` across a transport.
#[derive(Debug)]
pub struct InstallExecution {
    pub outcome: InstallOutcome,
    pub cause: Option<InstallCause>,
}
#[derive(Debug)]
pub enum InstallCause {
    Io(io::Error),
    Authority(AuthError),
    Authentication(ServiceUseError),
    /// Auth erased the request detail; retain it together with the same Original's observation.
    AuthenticationRequest {
        cause: ServiceUseError,
        observation: io::Error,
    },
    Projection(ProjectionError),
    Package(PackageError),
    Graph(graphit_data::DataReadError),
    Lance(graphit_data::lance_module_store::StoreError),
}
impl std::fmt::Display for InstallCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native installation failed")
    }
}
impl std::error::Error for InstallCause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Io(e) => e,
            Self::Authority(e) => e,
            Self::Authentication(e) => e,
            Self::AuthenticationRequest { cause, .. } => cause,
            Self::Projection(e) => e,
            Self::Package(e) => e,
            Self::Graph(e) => e,
            Self::Lance(e) => e,
        })
    }
}
/// All resources are selected by the process owner, never by incoming request data.
pub struct InstallResources<'a> {
    pub auth: &'a DaemonAuth,
    pub installation: &'a InstallationPaths,
    pub registry: &'a HubRegistry,
    pub policy: &'a HubResourcePolicy,
    pub graph_policy: LocalGraphPolicy,
}

fn verifier(
    caller: &AuthenticatedCaller<'_>,
    policy: &HubResourcePolicy,
) -> Result<BrokerVerifier, InstallCause> {
    let (service, revision) = caller
        .native_provider()
        .map_err(InstallCause::Authentication)?;
    let c = policy.content();
    BrokerVerifier::new(
        BrokerPolicy {
            endpoint: &service.endpoint,
            mcp_resource: service.mcp_resource.as_deref(),
            provider_revision: revision,
            network: ClientPolicy {
                origin: &service.endpoint,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[],
            },
        },
        NetworkLimits {
            request_body_bytes: c.max_input_bytes,
            response_body_bytes: c.max_input_bytes,
            url_bytes: c.max_scalar_bytes,
            header_bytes: c.max_input_bytes,
            header_count: c.max_collection_entries,
            max_in_flight: 1,
            max_idle_per_host: 1,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 1,
            max_certificate_bytes: c.max_input_bytes,
        },
        VerifierLimits {
            token_bytes: c.max_scalar_bytes,
            json: JsonLimits {
                bytes: c.max_input_bytes,
                nodes: c.max_nodes,
                depth: c.max_depth,
                scalar_bytes: c.max_scalar_bytes,
                collection_entries: c.max_collection_entries,
            },
            max_keys: c.max_collection_entries,
            metadata_ttl: Duration::from_secs(300),
            key_ttl: Duration::from_secs(300),
            unknown_key_cooldown: Duration::from_secs(1),
        },
    )
    .map_err(|e| InstallCause::Io(io::Error::other(e)))
}
fn barrier(
    caller: &AuthenticatedCaller<'_>,
    original: &RequestContext<'_>,
) -> Result<(), InstallCause> {
    original.snapshot().check().map_err(InstallCause::Io)?;
    for permission in [
        Permission::Configure,
        Permission::ManageCache,
        Permission::External(ExternalEffect::PublishedFilesystem),
    ] {
        caller
            .authorize_native_installation(permission)
            .map_err(|cause| {
                if matches!(&cause, ServiceUseError::Auth(AuthError::Request))
                    && let Err(observation) = original.snapshot().check()
                {
                    InstallCause::AuthenticationRequest { cause, observation }
                } else {
                    InstallCause::Authentication(cause)
                }
            })?;
    }
    original.snapshot().check().map_err(InstallCause::Io)
}
fn document_limits(policy: &HubResourcePolicy) -> io::Result<NativeDocumentFileLimits> {
    NativeDocumentFileLimits::new(
        policy.content().max_input_bytes,
        graphit_core::native_path::MAX_PATH_BYTES,
        usize::try_from(policy.limit(ResourceSetting::SnapshotDepth).value)
            .map_err(io::Error::other)?,
        policy.content().max_scalar_bytes,
    )
}
fn read_limits(policy: &HubResourcePolicy) -> io::Result<SourceReadLimits> {
    SourceReadLimits::new(
        policy.content().max_input_bytes,
        policy.content().max_scalar_bytes,
        policy.copy().total_file_bytes_limit(),
    )
}
fn validate_resources(
    resources: &InstallResources<'_>,
    original: &RequestContext<'_>,
) -> io::Result<()> {
    original.snapshot().check()?;
    let live = resources.auth.installation().live;
    let StoreOwner::Installation { installation_id } = live.owner() else {
        return Err(io::ErrorKind::PermissionDenied.into());
    };
    if live.owner_root() != resources.installation.root()
        || live.brand() != resources.installation.brand()
        || resources.registry.scope()
            != &(HubScope::Global {
                installation_id: installation_id.as_str().into(),
            })
        || resources.registry.path().parent() != Some(resources.installation.root())
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}
/// Create only normal cache ancestors under the already authenticated installation.
/// Captures detect cooperative replacement; pathname I/O is not a hostile filesystem boundary.
pub(crate) fn parent(
    installation: &InstallationPaths,
    policy: &HubResourcePolicy,
    destination: &Path,
    original: &RequestContext<'_>,
) -> io::Result<CapturedDocumentDirectory> {
    let limits = document_limits(policy)?;
    let mut root =
        CapturedDocumentDirectory::capture(installation.root(), limits, original.snapshot())?;
    let relative = destination
        .parent()
        .ok_or(io::ErrorKind::InvalidInput)?
        .strip_prefix(installation.root())
        .map_err(io::Error::other)?;
    let mut path = installation.root().to_owned();
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        root.revalidate(original.snapshot())?;
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                match fs::create_dir(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e),
                }
                let metadata = fs::symlink_metadata(&path)?;
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(io::ErrorKind::InvalidData.into());
                }
            }
            Err(e) => return Err(e),
        }
        root.revalidate(original.snapshot())?;
        // Reject replacement of any prior ancestor before selecting the next path.
        root = CapturedDocumentDirectory::capture(&path, limits, original.snapshot())?;
    }
    root.revalidate(original.snapshot())?;
    CapturedDocumentDirectory::capture(&path, limits, original.snapshot())
}
pub(crate) async fn admit(
    path: &Path,
    kind: PackageKind,
    resources: &InstallResources<'_>,
    original: &RequestContext<'_>,
) -> Result<(), InstallCause> {
    let policy = resources
        .policy
        .snapshot_policy(CopyPolicy::RejectFileLinks, [])
        .map_err(InstallCause::Io)?;
    let read = read_limits(resources.policy).map_err(InstallCause::Io)?;
    match kind {
        PackageKind::Ast => {
            let graph = CapturedLocalGraph::capture(
                &path.join("graph.icebug"),
                &policy,
                read,
                Arc::clone(original.snapshot()),
                resources.graph_policy,
            )
            .map_err(InstallCause::Graph)?;
            drop(graph);
            let source = path.join("graph.icebug/source.lance");
            match fs::symlink_metadata(&source) {
                Ok(_) => open_lance(&source, PackageKind::Ast, resources.policy, original).await?,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(InstallCause::Io(e)),
            }
        }
        PackageKind::Knowledge => {
            open_lance(
                &path.join("index.lance"),
                PackageKind::Knowledge,
                resources.policy,
                original,
            )
            .await?
        }
    }
    original.snapshot().check().map_err(InstallCause::Io)
}
// SDK schema types are inferred at the call sites; no duplicate schema or compatibility parser.
async fn open_lance(
    path: &Path,
    kind: PackageKind,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> Result<(), InstallCause> {
    let captured = CapturedDocumentDirectory::capture(
        path,
        document_limits(policy).map_err(InstallCause::Io)?,
        original.snapshot(),
    )
    .map_err(InstallCause::Io)?;
    let uri = captured
        .canonical_path()
        .to_str()
        .ok_or_else(|| InstallCause::Io(io::ErrorKind::InvalidInput.into()))?;
    let schema = match kind {
        PackageKind::Ast => graphit_ast::indexed_source::source_schema(),
        PackageKind::Knowledge => graphit_data::knowledge_index::physical_schema(),
    };
    let store = LanceModuleStore::open(uri, schema)
        .await
        .map_err(InstallCause::Lance)?;
    original.snapshot().check().map_err(InstallCause::Io)?;
    captured
        .revalidate(original.snapshot())
        .map_err(InstallCause::Io)?;
    drop(store);
    Ok(())
}
fn category(cause: &InstallCause, phase: InstallPhase) -> FailureCategory {
    match cause {
        InstallCause::Authentication(
            ServiceUseError::Profile(graphit_auth::profile::ProfileError::Cancelled)
            | ServiceUseError::Broker(graphit_auth::broker::BrokerError::Cancelled)
            | ServiceUseError::Store(graphit_auth::store::AuthStoreError::Core(
                graphit_core::auth_document::AuthDocumentError::Cancelled,
            ))
            | ServiceUseError::Store(graphit_auth::store::AuthStoreError::Profile(
                graphit_auth::profile::ProfileError::Cancelled,
            )),
        ) => return FailureCategory::Cancelled,
        InstallCause::Authentication(
            ServiceUseError::Profile(graphit_auth::profile::ProfileError::Deadline)
            | ServiceUseError::Broker(graphit_auth::broker::BrokerError::Deadline)
            | ServiceUseError::Store(graphit_auth::store::AuthStoreError::Core(
                graphit_core::auth_document::AuthDocumentError::Deadline,
            ))
            | ServiceUseError::Store(graphit_auth::store::AuthStoreError::Profile(
                graphit_auth::profile::ProfileError::Deadline,
            )),
        ) => return FailureCategory::Deadline,
        _ => {}
    }
    let io_kind = match cause {
        InstallCause::Io(e) => Some(e.kind()),
        InstallCause::AuthenticationRequest { observation, .. } => Some(observation.kind()),
        InstallCause::Package(PackageError::Io(e)) => Some(e.kind()),
        InstallCause::Graph(graphit_data::DataReadError::Source(e)) => Some(e.kind()),
        _ => None,
    };
    match io_kind {
        Some(io::ErrorKind::Interrupted) => return FailureCategory::Cancelled,
        Some(io::ErrorKind::TimedOut) => return FailureCategory::Deadline,
        Some(io::ErrorKind::AlreadyExists) => return FailureCategory::AlreadyExists,
        _ => {}
    }
    match cause {
        InstallCause::Authority(_)
        | InstallCause::Authentication(_)
        | InstallCause::AuthenticationRequest { .. } => FailureCategory::Authentication,
        InstallCause::Package(_) => FailureCategory::Package,
        InstallCause::Graph(_) => FailureCategory::Graph,
        InstallCause::Lance(_) => FailureCategory::Lance,
        _ if phase == InstallPhase::Input => FailureCategory::InvalidInput,
        _ if phase == InstallPhase::Registry => FailureCategory::Registry,
        _ if phase == InstallPhase::Delivery => FailureCategory::Delivery,
        _ if matches!(cause, InstallCause::Io(e) if e.kind() == io::ErrorKind::PermissionDenied) => {
            FailureCategory::PermissionDenied
        }
        _ => FailureCategory::Package,
    }
}
/// The operation derives its Runtime caller inside the same issuer and Original.
/// Returns known effects on every failure; a published payload is never removed or replayed.
pub async fn execute_install(
    resources: InstallResources<'_>,
    credential: &str,
    request: &InstallRequest,
    original: &RequestContext<'_>,
) -> InstallExecution {
    let mut outcome = InstallOutcome {
        producer: request.producer,
        id: request.id.clone(),
        version: request.version.clone(),
        kind: request.kind,
        destination: None,
        payload: PayloadEffect::NotPublished,
        registry: RegistryEffect::NotAttempted,
        failure: None,
    };
    let mut phase = InstallPhase::Input;
    let result = async {
        let deadline = original
            .snapshot()
            .required_deadline()
            .map_err(InstallCause::Io)?;
        if !request.input.is_absolute()
            || request
                .input
                .components()
                .any(|p| matches!(p, Component::CurDir | Component::ParentDir))
        {
            return Err(InstallCause::Io(io::ErrorKind::InvalidInput.into()));
        }
        let id = ArtifactId::new(&request.id, resources.policy.projection())
            .map_err(InstallCause::Projection)?;
        let version = ConcreteVersion::new(&request.version, resources.policy.projection())
            .map_err(InstallCause::Projection)?;
        validate_resources(&resources, original).map_err(InstallCause::Io)?;
        phase = InstallPhase::Authentication;
        let caller = resources
            .auth
            .issuer()
            .authenticate_runtime(credential, original)
            .map_err(InstallCause::Authority)?;
        let verifier = verifier(&caller, resources.policy)?;
        let caller = caller
            .authenticate_native(&verifier)
            .await
            .map_err(InstallCause::Authentication)?;
        barrier(&caller, original)?;
        phase = InstallPhase::Capture;
        let policy = resources
            .policy
            .snapshot_policy(CopyPolicy::RejectFileLinks, [])
            .map_err(InstallCause::Io)?;
        let source = CapturedSourceReader::capture(
            &request.input,
            graphit_fs::NodeKind::File,
            &policy,
            read_limits(resources.policy).map_err(InstallCause::Io)?,
            Arc::clone(original.snapshot()),
        )
        .map_err(InstallCause::Io)?;
        let kind = native_package::inspect(
            &source,
            request.kind,
            resources.policy.copy(),
            original.snapshot(),
        )
        .map_err(InstallCause::Package)?;
        outcome.kind = Some(kind);
        let artifact_type = match kind {
            PackageKind::Ast => ArtifactType::Ast,
            PackageKind::Knowledge => ArtifactType::Knowledge,
        };
        let source_ref = SourceRef::hub_cache(
            resources.installation,
            request.producer,
            artifact_type,
            id.clone(),
            None,
            version.clone(),
            resources.policy.projection(),
        )
        .map_err(InstallCause::Projection)?;
        let destination = source_ref.directory().as_path();
        outcome.destination = Some(destination.to_owned());
        barrier(&caller, original)?;
        let parent = parent(
            resources.installation,
            resources.policy,
            destination,
            original,
        )
        .map_err(InstallCause::Io)?;
        phase = InstallPhase::Stage;
        parent
            .revalidate(original.snapshot())
            .map_err(InstallCause::Io)?;
        let stage = native_package::stage_import(
            &source,
            Some(kind),
            destination,
            resources.policy.copy(),
            original.snapshot(),
        )
        .map_err(InstallCause::Package)?;
        phase = InstallPhase::Admission;
        let staged = CapturedSourceReader::capture(
            stage.path(),
            graphit_fs::NodeKind::Directory,
            &policy,
            read_limits(resources.policy).map_err(InstallCause::Io)?,
            Arc::clone(original.snapshot()),
        )
        .map_err(InstallCause::Io)?;
        admit(stage.path(), kind, &resources, original).await?;
        staged.revalidate_content().map_err(InstallCause::Io)?;
        drop(staged);
        phase = InstallPhase::Publish;
        barrier(&caller, original)?;
        parent
            .revalidate(original.snapshot())
            .map_err(InstallCause::Io)?;
        stage.publish().map_err(InstallCause::Package)?;
        outcome.payload = PayloadEffect::Published;
        phase = InstallPhase::Registry;
        barrier(&caller, original)?;
        parent
            .revalidate(original.snapshot())
            .map_err(InstallCause::Io)?;
        let entry = InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type,
            id,
            installations: vec![Installation {
                target: InstallationTarget::GlobalNative,
                original_source: source_ref,
                alias: None,
                entry_name: None,
                display: None,
                backend: InstalledBackend::Native,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        };
        let mut writer = resources
            .registry
            .writer(
                deadline.saturating_duration_since(std::time::Instant::now()),
                original.cancellation(),
            )
            .map_err(InstallCause::Io)?;
        barrier(&caller, original)?;
        outcome.registry = RegistryEffect::Unknown;
        match writer.put_metadata(entry.clone()) {
            Ok(_) => outcome.registry = RegistryEffect::Registered,
            Err(cause) => {
                // Keep the writer's lock while observing disk. A save error can follow replacement.
                if barrier(&caller, original).is_ok()
                    && parent.revalidate(original.snapshot()).is_ok()
                    && let Ok(document) = resources.registry.load()
                    && barrier(&caller, original).is_ok()
                    && parent.revalidate(original.snapshot()).is_ok()
                {
                    outcome.registry =
                        match document.get_version(artifact_type, &request.id, &version) {
                            Some(actual) if actual == &entry => RegistryEffect::Registered,
                            None => RegistryEffect::NotRegistered,
                            Some(_) => RegistryEffect::Unknown,
                        };
                }
                return Err(InstallCause::Io(cause));
            }
        }
        phase = InstallPhase::Delivery;
        barrier(&caller, original)?;
        Ok(())
    }
    .await;
    let cause = result.err();
    if let Some(cause) = &cause {
        outcome.failure = Some(InstallFailure {
            category: category(cause, phase),
            phase,
            message: "native installation did not complete".into(),
        });
    }
    InstallExecution { outcome, cause }
}
