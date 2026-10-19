//! Typed executable callers for the current authenticated Source service.
use crate::{
    ast_source::{self, SourceArguments},
    mcp_source::SourceProxy,
};
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use graphit_core::project_paths::ProjectPaths;
use graphit_core::{
    file_lock::Cancellation, native_document_file::NativeDocumentFileLimits,
    project_paths::InstallationPaths,
};
use graphit_daemon::source_discovery::{self, DiscoveryError};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use std::{io, net::SocketAddr, path::Path, time::Duration};
use tokio::io::{AsyncRead, AsyncWrite};

/// Read the selected installation's published address without starting a service.
pub async fn address(
    installation: &InstallationPaths,
    limits: NativeDocumentFileLimits,
    original: &RequestContext<'_>,
) -> io::Result<SocketAddr> {
    original.snapshot().check()?;
    let installation = installation.clone();
    let snapshot = original.snapshot().clone();
    let record = tokio::task::spawn_blocking(move || {
        source_discovery::read_discovery(&installation, limits, &snapshot)
    })
    .await
    .map_err(io::Error::other)?;
    original.snapshot().check()?;
    record
        .map_err(|error| match error {
            DiscoveryError::Operation(error) => error,
            other => io::Error::other(other),
        })?
        .map(|record| record.address())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Source service discovery is absent",
            )
        })
}

/// Consume arguments from the central Clap parser without reparsing or starting a service.
pub async fn call_source(
    installation: &InstallationPaths,
    client: &reqwest::Client,
    arguments: &SourceArguments,
    limits: NativeDocumentFileLimits,
    original: &RequestContext<'_>,
    output: &mut impl io::Write,
) -> io::Result<()> {
    let address = address(installation, limits, original).await?;
    ast_source::run_source(
        installation,
        address,
        client,
        arguments,
        limits,
        original,
        output,
    )
    .await
}

/// Maintain one real checkout on the discovered service before a PROJECT operation.
/// The caller supplies its captured address; this does not start a daemon.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub async fn maintain_checkout(
    installation: &InstallationPaths,
    address: SocketAddr,
    client: &reqwest::Client,
    project: &ProjectPaths,
    limits: (NativeDocumentFileLimits, ContentLimits),
    original: &RequestContext<'_>,
) -> io::Result<()> {
    original.check()?;
    let identity = graphit_core::project_identity::ProjectIdentityStore::at(project)
        .load()?
        .ok_or(io::ErrorKind::NotFound)?;
    original.check()?;
    let command = crate::foreground_project_control::ProjectCommand::MaintainCheckout {
        project_id: identity.id(),
        project_dir: graphit_core::native_path::NativePath::new(project.root().to_path_buf())?,
        // Automatic admission maintains the actual checkout, never a requested read view.
        version: String::new(),
        detached_base: String::new(),
        select: false,
    };
    let (status, response) = crate::foreground_project_control::send(
        installation,
        address,
        client,
        limits,
        &command,
        original,
    )
    .await?;
    if !status.is_success() {
        let kind = match status.as_u16() {
            403 => io::ErrorKind::PermissionDenied,
            404 => io::ErrorKind::NotFound,
            400 => io::ErrorKind::InvalidInput,
            504 => io::ErrorKind::TimedOut,
            _ => io::ErrorKind::Other,
        };
        return Err(io::Error::new(
            kind,
            format!("Source project startup {status}: {response}"),
        ));
    }
    Ok(())
}

/// The startup original is finite; the stdio owner has independent service cancellation.
#[allow(clippy::too_many_arguments)] // Separate startup, service and transport owners.
pub async fn stdio<R, W>(
    installation: &InstallationPaths,
    client: &reqwest::Client,
    policy: (NativeDocumentFileLimits, ContentLimits, Duration),
    current: Option<(&Path, Option<&str>)>,
    original: &RequestContext<'_>,
    service: &Cancellation,
    read: R,
    write: W,
) -> io::Result<()>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    if service.is_cancelled() {
        return Err(io::ErrorKind::Interrupted.into());
    }
    let address = address(installation, policy.0, original).await?;
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    let selected = current
        .map(|(root, version)| {
            let project = graphit_core::project_paths::ProjectPaths::at(root, installation)?;
            // Preserve the proxy's transport constraint before any maintain effects.
            project.root().to_str().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Source project root is not UTF-8",
                )
            })?;
            Ok::<_, io::Error>((project, version))
        })
        .transpose()?;
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    let current = selected
        .as_ref()
        .map(|(project, version)| (project.root(), *version));
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    if let Some((project, _)) = selected.as_ref() {
        maintain_checkout(
            installation,
            address,
            client,
            project,
            (policy.0, policy.1),
            original,
        )
        .await?;
    }
    let proxy = SourceProxy::new(
        installation.clone(),
        address,
        client.clone(),
        policy,
        current,
        original,
    )
    .await?;
    let serving = proxy.serve_stream(read, write);
    tokio::pin!(serving);
    let result = tokio::select! {
        result = &mut serving => result,
        () = service.cancelled() => {
            let joined = proxy.shutdown().await;
            let served = serving.await;
            // Await both SDK service and jobs before returning an interruption.
            joined?;
            match served {
                Ok(()) => Err(io::ErrorKind::Interrupted.into()),
                Err(error) => Err(io::Error::new(io::ErrorKind::Interrupted, error)),
            }
        }
    };
    let joined = proxy.shutdown().await;
    result.and(joined)
}
