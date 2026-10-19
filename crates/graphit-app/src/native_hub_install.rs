//! Explicit remote NativeRoot installation through the existing Source owner.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use graphit_core::project_identity::ProjectId;
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion,
    policy::{HubResourcePolicy, RequestContext},
};
use rmcp::model::CallToolRequestParams;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(clap::Args)]
pub struct InstallArguments {
    #[arg(long)]
    pub provider: String,
    #[arg(long)]
    pub producer: String,
    #[arg(long, value_enum)]
    pub kind: crate::hub_publish::Kind,
    #[arg(long)]
    pub id: String,
    #[arg(long)]
    pub version: String,
}

/// Selection data only. Runtime Native installation and HubRead are admitted by Source.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Selection {
    pub provider: String,
    pub producer: ProjectId,
    pub artifact_type: ArtifactType,
    pub id: String,
    pub version: String,
}
impl Selection {
    pub(crate) fn validate(&self, policy: &HubResourcePolicy) -> io::Result<()> {
        if self.provider.is_empty()
            || self.provider.trim() != self.provider
            || self.provider.chars().any(char::is_control)
            || self.provider.len() > policy.content().max_scalar_bytes
            || !matches!(
                self.artifact_type,
                ArtifactType::Ast | ArtifactType::Knowledge
            )
            || self.version.eq_ignore_ascii_case("latest")
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        ArtifactId::new(&self.id, policy.projection()).map_err(io::Error::other)?;
        ConcreteVersion::new(&self.version, policy.projection()).map_err(io::Error::other)?;
        Ok(())
    }
}

/// Validate the literal selector before daemon startup or any remote request.
pub fn prepare(
    arguments: &InstallArguments,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CallToolRequestParams> {
    original.check()?;
    let selection = Selection {
        provider: arguments.provider.clone(),
        producer: arguments.producer.parse().map_err(io::Error::other)?,
        artifact_type: match arguments.kind {
            crate::hub_publish::Kind::Ast => ArtifactType::Ast,
            crate::hub_publish::Kind::Knowledge => ArtifactType::Knowledge,
        },
        id: arguments.id.clone(),
        version: arguments.version.clone(),
    };
    selection.validate(policy)?;
    let args = serde_json::json!({"installation":selection});
    if serde_json::to_vec(&args).map_err(io::Error::other)?.len()
        > original.content_limits().max_input_bytes
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    original.check()?;
    Ok(CallToolRequestParams::new("graphit_hub_install_native")
        .with_arguments(args.as_object().unwrap().clone()))
}

pub(crate) struct InstallInput<'a, 'r> {
    pub selection: &'a Selection,
    pub caller: &'a graphit_auth::AuthenticatedCaller<'r>,
    pub lease: &'a graphit_auth::ServiceCredentialLease<'r>,
    pub storage: &'a crate::native_storage_credentials::BrokerStorage,
    pub installation: &'a graphit_core::project_paths::InstallationPaths,
    pub registry: &'a graphit_hub::registry::HubRegistry,
    pub policy: &'a HubResourcePolicy,
}

/// Read an admitted immutable release and register its compact source/display data last.
/// No payload is downloaded to the local cache; each reader still requires its own admission.
pub(crate) async fn install(
    input: InstallInput<'_, '_>,
    original: &RequestContext<'_>,
) -> io::Result<serde_json::Value> {
    install_release(input, None, original).await
}

pub(crate) async fn update(
    input: InstallInput<'_, '_>,
    previous: &graphit_hub::registry::Installation,
    original: &RequestContext<'_>,
) -> io::Result<serde_json::Value> {
    install_release(input, Some(previous), original).await
}

async fn install_release(
    input: InstallInput<'_, '_>,
    previous: Option<&graphit_hub::registry::Installation>,
    original: &RequestContext<'_>,
) -> io::Result<serde_json::Value> {
    use graphit_auth::{ExternalEffect, Permission, ServiceBindingView, ServicePurpose};
    use graphit_hub::{
        InstallationTarget, SourceRef,
        native_root::{NativeRole, NativeRoot, NativeSnapshot},
        registry::{
            ArtifactDisplayMetadata, HubScope, Installation, InstalledArtifact, InstalledBackend,
            InstalledPayload,
        },
        remote_metadata::RemoteArtifactManifest,
        remote_native::RemoteNativeRecord,
        remote_storage::{registry_key, release_prefix},
    };
    use object_store::path::Path;
    use sha2::{Digest, Sha256};
    input.selection.validate(input.policy)?;
    let check = || -> io::Result<()> {
        original.check()?;
        for permission in [
            Permission::Configure,
            Permission::ManageCache,
            Permission::External(ExternalEffect::PublishedFilesystem),
        ] {
            input
                .caller
                .authorize_native_installation(permission)
                .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?;
        }
        input.lease.check().map_err(io::Error::other)?;
        input.storage.check().map_err(io::Error::from)?;
        if !std::sync::Arc::ptr_eq(
            input.lease.original().map_err(io::Error::other)?.snapshot(),
            original.snapshot(),
        ) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        match input.lease.binding().map_err(io::Error::other)? {
            ServiceBindingView::Broker {
                provider,
                purpose: ServicePurpose::HubRead,
                ..
            } if provider == input.selection.provider => Ok(()),
            _ => Err(io::ErrorKind::PermissionDenied.into()),
        }
    };
    check()?;
    let id = ArtifactId::new(&input.selection.id, input.policy.projection())
        .map_err(io::Error::other)?;
    let version = ConcreteVersion::new(&input.selection.version, input.policy.projection())
        .map_err(io::Error::other)?;
    let limits = crate::native_publication::metadata_limits(
        input.policy.content(),
        input.policy.projection(),
    );
    let object_key = |relative: &str| -> io::Result<Path> {
        Path::parse(if input.storage.prefix.is_empty() {
            relative.to_owned()
        } else {
            format!("{}/{relative}", input.storage.prefix)
        })
        .map_err(io::Error::other)
    };
    let catalog_path = object_key(&registry_key(
        input.selection.producer,
        input.selection.artifact_type,
        &id,
    )?)?;
    let store: std::sync::Arc<dyn object_store::ObjectStore> = input.storage.store.clone();
    let catalog_bytes = crate::remote_native_mount::read_small(
        &store,
        &catalog_path,
        limits.bytes,
        original.snapshot(),
    )
    .await
    .map_err(io::Error::other)?;
    check()?;
    let manifest = RemoteArtifactManifest::decode(&catalog_bytes, limits, original)
        .map_err(io::Error::other)?;
    if manifest.producer() != input.selection.producer
        || manifest.artifact_type() != input.selection.artifact_type
        || manifest.id() != &id
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            InvalidRelease::CatalogTuple,
        ));
    }
    let requested_version = if previous.is_some() {
        manifest.latest().ok_or(io::ErrorKind::NotFound)?
    } else {
        &version
    };
    let selected = manifest
        .versions()
        .iter()
        .find(|entry| entry.version() == requested_version)
        .ok_or(io::ErrorKind::NotFound)?;
    let version = selected.version().clone();
    let digest = *selected.hash();
    let release = release_prefix(
        input.selection.producer,
        input.selection.artifact_type,
        &id,
        &version,
        &digest,
    )?;
    let root_path = object_key(&format!("{release}native.json"))?;
    let root_bytes = crate::remote_native_mount::read_small(
        &store,
        &root_path,
        limits.bytes,
        original.snapshot(),
    )
    .await
    .map_err(io::Error::other)?;
    check()?;
    if <[u8; 32]>::from(Sha256::digest(&root_bytes)) != digest {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            InvalidRelease::RootDigest,
        ));
    }
    let root = NativeRoot::decode(&root_bytes, limits, original).map_err(io::Error::other)?;
    if root.producer() != input.selection.producer
        || root.artifact_type() != input.selection.artifact_type
        || root.id() != &id
        || root.version() != &version
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            InvalidRelease::RootTuple,
        ));
    }
    let session = crate::remote_native_mount::admitted_session(&input.storage.bucket, &store);
    for component in root.components() {
        let component_path = object_key(&format!("{release}{}", component.path))?;
        let path = Path::parse(format!(
            "{component_path}/{}",
            component.snapshot.manifest_path()
        ))
        .map_err(io::Error::other)?;
        if let NativeSnapshot::Lance {
            dataset_version, ..
        } = &component.snapshot
        {
            let mut uri = url::Url::parse(&format!("s3://{}/", input.storage.bucket))
                .map_err(io::Error::other)?;
            uri.path_segments_mut()
                .map_err(|_| io::ErrorKind::InvalidInput)?
                .clear()
                .extend(component_path.as_ref().split('/'));
            let open = async {
                match component.role {
                    NativeRole::EntitySearch => {
                        let (corpus, location) =
                            graphit_data::lance_branch_store::BranchSnapshot::capture_native(
                                uri.as_str(),
                                std::sync::Arc::clone(&session),
                                *dataset_version,
                            )
                            .await
                            .map_err(io::Error::other)?;
                        graphit_ast::corpus::validate_schema(corpus.schema())?;
                        Ok::<_, io::Error>(location)
                    }
                    role => {
                        let schema = match role {
                            NativeRole::Source => graphit_ast::indexed_source::source_schema(),
                            NativeRole::Wiki => graphit_data::knowledge_index::physical_schema(),
                            _ => return Err(io::ErrorKind::InvalidData.into()),
                        };
                        let (_, location) = graphit_data::lance_module_store::LanceModuleStore::open_native_snapshot(
                            uri.as_str(), schema, std::sync::Arc::clone(&session), *dataset_version,
                        ).await.map_err(io::Error::other)?;
                        Ok(location)
                    }
                }
            };
            let location = tokio::select! {
                biased;
                () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
                () = tokio::time::sleep_until(original.snapshot().required_deadline()?.into()) => return Err(io::ErrorKind::TimedOut.into()),
                location = open => location?,
            };
            check()?;
            if location.version != *dataset_version || location.path != path {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    InvalidRelease::ManifestLocation {
                        role: component.role,
                        expected_version: *dataset_version,
                        actual_version: location.version,
                        expected_path: path.to_string(),
                        actual_path: location.path.to_string(),
                    },
                ));
            }
        }
        let actual = crate::remote_native_mount::hash_object(&store, &path, original.snapshot())
            .await
            .map_err(io::Error::other)?;
        check()?;
        if &actual != component.snapshot.manifest_sha256() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                InvalidRelease::ManifestDigest {
                    role: component.role,
                },
            ));
        }
    }
    let display = ArtifactDisplayMetadata::new(
        manifest.name(),
        manifest.description(),
        manifest.tags(),
        input.registry.limits(),
    )?;
    let source = SourceRef::hub_cache(
        input.installation,
        input.selection.producer,
        input.selection.artifact_type,
        id.clone(),
        None,
        version.clone(),
        input.policy.projection(),
    )
    .map_err(io::Error::other)?;
    let record = RemoteNativeRecord::new(
        &input.selection.provider,
        &input.storage.bucket,
        &input.storage.region,
        input.storage.endpoint.as_deref(),
        &input.storage.prefix,
        digest,
        input.policy.projection(),
    )?;
    let entry = InstalledArtifact {
        consumer: graphit_core::artifact_store::PublicationSlot::Scope,
        artifact_type: input.selection.artifact_type,
        id,
        installations: vec![Installation {
            target: InstallationTarget::GlobalNative,
            original_source: source,
            display: Some(display),
            alias: None,
            entry_name: None,
            backend: InstalledBackend::RemoteNative(record),
            payload: InstalledPayload::Original {
                selection_binding: None,
            },
        }],
    };
    // The actual Source composition supplies the installation identity. Scope ownership
    // and the issuer's Native installation proof remain independently checked there.
    if !matches!(input.registry.scope(), HubScope::Global { .. })
        || input.registry.path().parent() != Some(input.installation.root())
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    check()?;
    let mut writer = input.registry.writer(
        original.remaining()?.ok_or(io::ErrorKind::TimedOut)?,
        original.cancellation(),
    )?;
    check()?;
    let saved = match previous {
        Some(previous) => writer.replace_metadata_version(previous, entry.clone()),
        None => writer.put_metadata(entry.clone()),
    };
    let changed = match saved {
        Ok(changed) => changed,
        Err(cause) => {
            let observed = if check().is_ok() {
                input.registry.load().ok().map(|document| {
                    match document.get_version(
                        input.selection.artifact_type,
                        &input.selection.id,
                        &version,
                    ) {
                        Some(current) if current == &entry => {
                            crate::native_install::RegistryEffect::Registered
                        }
                        None => crate::native_install::RegistryEffect::NotRegistered,
                        Some(_) => crate::native_install::RegistryEffect::Unknown,
                    }
                })
            } else {
                None
            };
            return Err(io::Error::new(
                cause.kind(),
                InstallationFailure {
                    registry: observed.unwrap_or(crate::native_install::RegistryEffect::Unknown),
                    cause,
                },
            ));
        }
    };
    if let Err(cause) = check() {
        return Err(io::Error::new(
            cause.kind(),
            InstallationFailure {
                registry: crate::native_install::RegistryEffect::Registered,
                cause,
            },
        ));
    }
    Ok(
        serde_json::json!({"status":"installed","producer":input.selection.producer,"artifact_type":input.selection.artifact_type,"id":input.selection.id,"version":version,"changed":changed}),
    )
}

#[derive(Debug)]
pub(crate) struct InstallationFailure {
    pub registry: crate::native_install::RegistryEffect,
    cause: io::Error,
}
impl std::fmt::Display for InstallationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("remote native installation registry operation failed")
    }
}
impl std::error::Error for InstallationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// Only stable, non-secret integrity diagnostics cross the MCP boundary.
#[derive(Debug, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(crate) enum InvalidRelease {
    CatalogTuple,
    RootDigest,
    RootTuple,
    ManifestLocation {
        role: graphit_hub::native_root::NativeRole,
        expected_version: u64,
        actual_version: u64,
        expected_path: String,
        actual_path: String,
    },
    ManifestDigest {
        role: graphit_hub::native_root::NativeRole,
    },
}
impl std::fmt::Display for InvalidRelease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("remote native release integrity mismatch")
    }
}
impl std::error::Error for InvalidRelease {}
