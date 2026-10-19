//! CLI client for administration on the daemon's actual existing issuer/key.
use crate::{
    admin_auth::{Event, ROUTE},
    ast_source::runtime_headers,
    auth_cli::Command,
};
use graphit_core::{
    file_lock::FileLock, native_document_file::NativeDocumentFileLimits,
    project_paths::InstallationPaths,
};
use graphit_daemon::source_discovery::{DiscoveryError, read_discovery};
use graphit_hub::policy::RequestContext;
use std::{
    io::{self, Write},
    process::ExitCode,
};
use zeroize::Zeroizing;

/// None means discovery is absent or its installation singleton is no longer held.
/// The standalone entry must acquire that singleton again; transport/auth failures remain errors.
#[allow(clippy::too_many_arguments)] // Actual transport, original and output owners remain separate.
pub async fn run_if_active(
    command: &Command,
    installation: &InstallationPaths,
    client: &reqwest::Client,
    limits: NativeDocumentFileLimits,
    max_output_bytes: usize,
    original: &RequestContext<'_>,
    output: &mut dyn Write,
    diagnostic: &mut dyn Write,
    mut announce: impl FnMut(&str) -> io::Result<()>,
) -> io::Result<Option<ExitCode>> {
    let deadline = original.snapshot().required_deadline()?;
    let paths = installation.clone();
    let snapshot = original.snapshot().clone();
    let discovery = tokio::task::spawn_blocking(move || {
        let discovery = read_discovery(&paths, limits, &snapshot).map_err(|e| match e {
            DiscoveryError::Operation(e) => e,
            other => io::Error::other(other),
        })?;
        snapshot.check()?;
        // Discovery can survive an interrupted owner. This nonmutating kernel probe
        // is only routing evidence: DaemonAuth::start still owns exclusive acquisition,
        // identity validation and runtime-key recovery before any local operation.
        if discovery.is_some() && !FileLock::is_locked(paths.global_path(&["daemon.pid"])?)? {
            snapshot.check()?;
            return Ok(None);
        }
        snapshot.check()?;
        Ok::<_, io::Error>(discovery)
    })
    .await
    .map_err(io::Error::other)??;
    original.snapshot().check()?;
    let Some(discovery) = discovery else {
        return Ok(None);
    };
    let headers = runtime_headers(installation, limits, original).await?;
    let payload = Zeroizing::new(serde_json::to_vec(command)?);
    let request = client
        .post(format!("http://{}{ROUTE}", discovery.address()))
        .headers(headers.into_iter().collect())
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(payload.to_vec());
    let operation = async {
        let mut response = request.send().await.map_err(io::Error::other)?;
        if !response.status().is_success() {
            return Err(io::Error::new(
                if matches!(response.status().as_u16(), 401 | 403) {
                    io::ErrorKind::PermissionDenied
                } else {
                    io::ErrorKind::Other
                },
                format!("active Auth service returned HTTP {}", response.status()),
            ));
        }
        let mut buffer = Vec::new();
        let mut result = None;
        while let Some(chunk) = response.chunk().await.map_err(io::Error::other)? {
            original.snapshot().check()?;
            buffer.extend_from_slice(&chunk);
            // Limit real response storage, including a malformed unterminated frame.
            if buffer.len() > max_output_bytes {
                return Err(io::Error::other("Auth response exceeds output limit"));
            }
            while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                let event: Event = serde_json::from_slice(&buffer[..end])?;
                buffer.drain(..=end);
                if result.is_some() {
                    return Err(io::Error::other("unexpected Auth event after result"));
                }
                match event {
                    Event::AuthorizationUrl { url } => announce(&url)?,
                    Event::DeviceAuthorization {
                        verification_uri,
                        verification_uri_complete,
                        user_code,
                    } => {
                        writeln!(
                            diagnostic,
                            "Open {verification_uri} and enter code {user_code}"
                        )?;
                        announce(
                            verification_uri_complete
                                .as_deref()
                                .unwrap_or(&verification_uri),
                        )?;
                    }
                    Event::Read { value } => {
                        serde_json::to_writer(&mut *output, &value)?;
                        output.write_all(b"\n")?;
                        result = Some(ExitCode::SUCCESS);
                    }
                    Event::Publication {
                        effect,
                        failure,
                        cleanup_effect,
                        cleanup_failure,
                        pending,
                    } => {
                        writeln!(output, "{effect}")?;
                        let failed = failure.is_some() || cleanup_failure.is_some() || pending;
                        if failed {
                            writeln!(
                                diagnostic,
                                "authentication publication: effect={effect}; failure={failure:?}; cleanup={cleanup_effect}; cleanup_failure={cleanup_failure:?}; pending={pending}"
                            )?;
                        }
                        result = Some(if failed {
                            ExitCode::FAILURE
                        } else {
                            ExitCode::SUCCESS
                        });
                    }
                    Event::Failure {
                        message,
                        remote_effect,
                    } => {
                        writeln!(
                            diagnostic,
                            "authentication: {message}; remote_effect={remote_effect:?}"
                        )?;
                        result = Some(ExitCode::FAILURE);
                    }
                }
            }
        }
        original.snapshot().check()?;
        if !buffer.is_empty() {
            return Err(io::Error::other("truncated Auth response"));
        }
        result
            .map(Some)
            .ok_or_else(|| io::Error::other("Auth service closed without a result"))
    };
    tokio::select! {
        result = operation => result,
        () = original.snapshot().cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(deadline.into()) => Err(io::ErrorKind::TimedOut.into()),
    }
}
