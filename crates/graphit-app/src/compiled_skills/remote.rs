//! Keep the admitted FILE lease alive until the selected skill's publication finishes.
use super::HubSelection;
use crate::{managed_roles::InstallResources, remote_file_install::FileTransport};
use graphit_auth::{Obligation, Permission, ServiceMode};
use graphit_core::{
    artifact_store::{ArtifactStore, CapturedPublicationScope, StoreOwner},
    module_settings::Module,
};
use graphit_daemon::broker_verifiers::BrokerVerifierCache;
use graphit_hub::{RemoteFileOrigin, SourceRef, policy::RequestContext};
use std::{io, sync::Arc};

/// Existing Source transport owners, captured before the blocking publication worker.
/// This value contains no credential and does not grant access to a source.
pub struct NativeSkillTransport {
    pub(crate) http: reqwest::Client,
    pub(crate) verifiers: Option<Arc<BrokerVerifierCache>>,
    pub(crate) allow_loopback_http: bool,
    pub(crate) runtime: tokio::runtime::Handle,
}
impl NativeSkillTransport {
    pub(super) fn with_source<T>(
        &self,
        resources: &InstallResources<'_>,
        credential: &str,
        selection: &HubSelection,
        source: &SourceRef,
        original: &RequestContext<'_>,
        operation: impl FnOnce(&dyn Fn() -> io::Result<()>) -> io::Result<T>,
    ) -> io::Result<T> {
        self.runtime.block_on(async {
            original.check()?;
            let issuer = resources.auth.issuer();
            let caller = issuer
                .authenticate_runtime(credential, original)
                .map_err(io::Error::other)?;
            let origin = source
                .remote_file_origin()
                .ok_or(io::ErrorKind::InvalidInput)?;
            if matches!(origin, RemoteFileOrigin::Broker { .. })
                && selection.identity_provider.is_some()
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let mut service = match origin {
                RemoteFileOrigin::Broker { provider } => {
                    issuer.capture_hub_service(&caller, provider)
                }
                RemoteFileOrigin::Standalone { provider, profile } => {
                    issuer.capture_standalone_s3_service(&caller, provider, profile.as_deref())
                }
            }
            .map_err(io::Error::other)?;
            let verifier = if service.mode().map_err(io::Error::other)? == ServiceMode::BrokerNative
            {
                Some(
                    self.verifiers
                        .as_ref()
                        .ok_or(io::ErrorKind::NotConnected)?
                        .get(&mut service)
                        .map_err(io::Error::other)?,
                )
            } else {
                None
            };
            let lease = match verifier.as_deref() {
                Some(verifier) => service
                    .native_credential(verifier)
                    .await
                    .map_err(io::Error::other)?,
                None => service.credential().map_err(io::Error::other)?,
            };
            let mut identity = selection
                .identity_provider
                .as_deref()
                .map(|provider| issuer.capture_hub_service(&caller, provider))
                .transpose()
                .map_err(io::Error::other)?;
            let identity_verifier = if let Some(identity) = identity.as_mut() {
                if identity.mode().map_err(io::Error::other)? != ServiceMode::BrokerNative {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
                Some(
                    self.verifiers
                        .as_ref()
                        .ok_or(io::ErrorKind::NotConnected)?
                        .get(identity)
                        .map_err(io::Error::other)?,
                )
            } else {
                None
            };
            let identity_lease = match (identity.as_mut(), identity_verifier.as_deref()) {
                (Some(identity), Some(verifier)) => Some(
                    identity
                        .native_credential(verifier)
                        .await
                        .map_err(io::Error::other)?,
                ),
                _ => None,
            };
            let subject = match &identity_lease {
                Some(identity) => identity.user_subject().map_err(io::Error::other)?,
                None => lease.user_subject().map_err(io::Error::other)?,
            };
            let transport = FileTransport::open(
                &lease,
                subject,
                source.producer(),
                &self.http,
                self.allow_loopback_http,
            )
            .await?;
            let policy = &resources.policy.native_install.hub;
            transport.authorize(policy).await?;
            // Any renewal precedes the management proof used during capture/publication.
            let manager = issuer
                .authenticate_runtime(credential, original)
                .map_err(io::Error::other)?;
            let check = || -> io::Result<()> {
                original.check()?;
                transport.check()?;
                if manager
                    .authorize(Permission::Read(Module::Hub))
                    .map_err(io::Error::other)?
                    != Obligation::Unconditional
                {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
                Ok(())
            };
            check()?;
            let installation = &resources.policy.installation;
            let live = resources.auth.installation().live;
            let StoreOwner::Installation { installation_id } = live.owner() else {
                return Err(io::ErrorKind::PermissionDenied.into());
            };
            if live.owner_root() != installation.root() {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            let scope = CapturedPublicationScope::installation(
                installation,
                installation_id.clone(),
                [installation.root().to_owned()],
                resources.policy.scope,
                original.snapshot(),
            )?;
            let cache = ArtifactStore::new(scope, policy.artifacts());
            if !transport.verify_cache(source, &cache, source.directory().as_path())? {
                return Err(io::ErrorKind::NotFound.into());
            }
            let result = operation(&check)?;
            check()?;
            if !transport.verify_cache(source, &cache, source.directory().as_path())? {
                return Err(io::ErrorKind::Interrupted.into());
            }
            check()?;
            Ok(result)
        })
    }
}
