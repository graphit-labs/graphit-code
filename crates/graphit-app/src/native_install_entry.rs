//! Installation-only offline composition with ordinary ownership and real Native authority.
use crate::{
    native_install::{self, InstallExecution, InstallOutcome, InstallRequest, InstallResources},
    native_install_http::NativeInstallPolicy,
};
use graphit_auth::{
    AdminSessionPolicy, ExternalEffect, IssuerAuthorityPolicy, IssuerLimits, Permission,
    ServiceAdmissionLimits, ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::ProfileLimits,
};
use graphit_core::{
    artifact_store::StoreOwner, auth_document::AuthDocumentLimits, project_paths::InstallationPaths,
};
use graphit_daemon::auth_startup::{AuthStartupFailure, DaemonAuth, StartupPolicy};
use graphit_hub::{
    policy::{HubResourcePolicy, RequestContext, ResourceSetting},
    registry::HubRegistry,
};
use std::{
    fmt,
    io::{self, Write},
    time::Duration,
};

enum Cause {
    Io(io::Error),
    Startup(AuthStartupFailure),
}
pub struct InstallEntryFailure {
    cause: Cause,
}
impl fmt::Debug for InstallEntryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl fmt::Display for InstallEntryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("offline native installation failed")
    }
}
impl std::error::Error for InstallEntryFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match &self.cause {
            Cause::Io(e) => e,
            Cause::Startup(e) => e,
        })
    }
}
impl From<io::Error> for InstallEntryFailure {
    fn from(cause: io::Error) -> Self {
        Self {
            cause: Cause::Io(cause),
        }
    }
}
fn number(policy: &HubResourcePolicy, setting: ResourceSetting) -> io::Result<usize> {
    usize::try_from(policy.limit(setting).value).map_err(io::Error::other)
}
fn startup(policy: &HubResourcePolicy) -> io::Result<StartupPolicy> {
    let paths = number(policy, ResourceSetting::ContentInputBytes)?;
    let bytes = number(policy, ResourceSetting::ContentInputBytes)?;
    let scalar = number(policy, ResourceSetting::ContentScalarBytes)?;
    let entries = number(policy, ResourceSetting::ContentCollectionEntries)?;
    let source = || SourceAuthorityPolicy {
        ordinary: &[],
        administrative: &[],
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(15 * 60),
            maximum: Duration::from_secs(60 * 60),
        },
    };
    let authority = IssuerAuthorityPolicy::new(
        source(),
        source(),
        IssuerLimits {
            max_permissions: entries,
            max_scope_bytes: paths,
            max_parent_bearer_bytes: scalar,
        },
        ServiceAuthorityInput {
            rules: &[],
            limits: ServiceAdmissionLimits {
                max_rules: entries,
                max_binding_bytes: paths,
            },
            native_refresh_margin: Duration::ZERO,
        },
    )
    .and_then(|policy| {
        policy.with_native_installation_permissions(&[
            Permission::Configure,
            Permission::ManageCache,
            Permission::External(ExternalEffect::PublishedFilesystem),
        ])
    })
    .map_err(io::Error::other)?;
    Ok(StartupPolicy {
        document: AuthDocumentLimits::new(bytes).map_err(io::Error::other)?,
        profiles: ProfileLimits {
            bytes,
            nodes: number(policy, ResourceSetting::ContentNodes)?,
            scalar_bytes: scalar,
            collection_entries: entries,
            providers: entries,
            profiles: entries,
        },
        authority,
        ttl: TtlPolicy {
            ttl: Duration::from_secs(30 * 60),
            maximum: Duration::from_secs(24 * 60 * 60),
            refresh_margin: Duration::from_secs(60),
        },
    })
}
/// Call only after genuine discovery absence. A singleton race is an error, without replay.
pub async fn execute(
    installation: &InstallationPaths,
    request: &InstallRequest,
    policy: &NativeInstallPolicy,
    original: &RequestContext<'_>,
) -> Result<InstallExecution, InstallEntryFailure> {
    original.snapshot().check()?;
    let auth =
        DaemonAuth::start(installation, &[], startup(&policy.hub)?, original).map_err(|cause| {
            InstallEntryFailure {
                cause: Cause::Startup(cause),
            }
        })?;
    let StoreOwner::Installation { installation_id } = auth.installation().owner() else {
        return Err(io::Error::from(io::ErrorKind::InvalidData).into());
    };
    let registry = HubRegistry::global(installation, installation_id, policy.hub.hub())?;
    let checked = auth
        .runtime()
        .check(original.snapshot())
        .map_err(io::Error::other)?;
    let credential = std::str::from_utf8(
        checked
            .secret(original.snapshot())
            .map_err(io::Error::other)?,
    )
    .map_err(io::Error::other)?;
    Ok(native_install::execute_install(
        InstallResources {
            auth: &auth,
            installation,
            registry: &registry,
            policy: &policy.hub,
            graph_policy: policy.graph,
        },
        credential,
        request,
        original,
    )
    .await)
}

#[derive(Debug)]
pub struct InstallDeliveryFailure {
    cause: io::Error,
    payload: crate::native_install::PayloadEffect,
    registry: crate::native_install::RegistryEffect,
}
impl fmt::Display for InstallDeliveryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "native installation output failed (payload={:?}, registry={:?})",
            self.payload, self.registry
        )
    }
}
impl std::error::Error for InstallDeliveryFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
/// A failed local output retains observed effects; it never rewrites the terminal or repeats work.
pub fn write_outcome(
    outcome: &InstallOutcome,
    maximum: usize,
    original: &RequestContext<'_>,
    output: &mut dyn Write,
) -> Result<(), InstallDeliveryFailure> {
    let report_check = || -> io::Result<()> {
        match original.snapshot().check() {
            Ok(()) => Ok(()),
            Err(error)
                if matches!(
                    (error.kind(), outcome.failure.as_ref().map(|e| e.category)),
                    (
                        io::ErrorKind::Interrupted,
                        Some(crate::native_install::FailureCategory::Cancelled)
                    ) | (
                        io::ErrorKind::TimedOut,
                        Some(crate::native_install::FailureCategory::Deadline)
                    )
                ) =>
            {
                Ok(())
            }
            Err(error) => Err(error),
        }
    };
    let result = (|| -> io::Result<()> {
        report_check()?;
        let mut bytes = serde_json::to_vec(outcome).map_err(io::Error::other)?;
        if bytes
            .len()
            .checked_add(1)
            .is_none_or(|length| length > maximum)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "native installation terminal exceeds output limit",
            ));
        }
        bytes.push(b'\n');
        report_check()?;
        output.write_all(&bytes)?;
        output.flush()?;
        Ok(())
    })();
    result.map_err(|cause| InstallDeliveryFailure {
        cause,
        payload: outcome.payload,
        registry: outcome.registry,
    })
}
