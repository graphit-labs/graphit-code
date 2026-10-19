//! Explicit product Coordinator publication through the active Rust daemon.
use super::{CoordinatorOutcome, CoordinatorRequest, ROUTE};
use crate::ast_source::runtime_headers;
use graphit_core::{
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_daemon::source_discovery::read_discovery;
use graphit_hub::policy::{HubResourcePolicy, RequestContext};
use std::{
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};
#[derive(clap::Args)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: Command,
}
#[derive(clap::Subcommand)]
pub enum Command {
    #[command(visible_alias = "sync")]
    Install(Selection),
    /// Withdraw the owned Coordinator, its hooks and MCP selection.
    Remove {
        #[arg(long, value_parser = crate::project_hook_install::client::parse_binding)]
        binding: graphit_hub::NativeBinding,
    },
}
#[derive(clap::Args)]
pub struct Selection {
    /// Native host schema for this publication.
    #[arg(long, value_parser = crate::project_hook_install::client::parse_binding)]
    pub binding: graphit_hub::NativeBinding,
    /// Override the bundled versioned Graphit/Coordinator artifact directory.
    #[arg(long)]
    pub source_dir: Option<PathBuf>,
}
fn root_transport() -> io::Result<()> {
    // Transient Role transport inputs can only deny the Root administrative path.
    if [
        "GRAPHIT_ROLE_BEARER",
        "GRAPHIT_ROLE_REFERENCE",
        "GRAPHIT_ROLE_SELECTION",
    ]
    .iter()
    .any(|key| std::env::var_os(key).is_some())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Root Coordinator installation is unavailable in a managed-role transport context",
        ));
    }

    Ok(())
}

/// Validate and serialize one request before daemon startup or transport.
pub fn prepare(
    arguments: Arguments,
    project: &ProjectPaths,
    version: Option<String>,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<Vec<u8>> {
    original.check()?;
    let _deadline = original.snapshot().required_deadline()?;
    root_transport()?;
    let (operation, binding, source_dir) = match arguments.command {
        Command::Install(selection) => {
            let directory = match selection.source_dir {
                Some(directory) => directory,
                None => std::env::current_exe()?
                    .parent()
                    .ok_or(io::ErrorKind::NotFound)?
                    .join("graphit-runtime/agents/coordinator"),
            };
            (
                crate::project_hook_install::HookOperation::Install,
                selection.binding,
                Some(std::fs::canonicalize(directory)?),
            )
        }
        Command::Remove { binding } => (
            crate::project_hook_install::HookOperation::Remove,
            binding,
            None,
        ),
    };
    let input = CoordinatorRequest {
        operation,
        binding,
        kimi_config_dir: crate::project_hook_install::client::kimi_config_dir(binding)?,
        source_dir,
        project_dir: project.root().into(),
        project_version: version,
    };
    let body = serde_json::to_vec(&input).map_err(io::Error::other)?;
    if body.len() > policy.content().max_input_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    original.check()?;
    Ok(body)
}

/// Send the prepared request through the active daemon; this never starts it.
pub async fn run(
    body: Vec<u8>,
    installation: &InstallationPaths,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
    output: &mut dyn Write,
) -> io::Result<ExitCode> {
    original.check()?;
    let deadline = original.snapshot().required_deadline()?;
    root_transport()?;
    if body.len() > policy.content().max_input_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let limits = NativeDocumentFileLimits::new(
        policy.content().max_input_bytes,
        graphit_core::native_path::MAX_PATH_BYTES,
        policy.content().max_depth,
        policy.content().max_scalar_bytes,
    )?;
    let paths = installation.clone();
    let snapshot = original.snapshot().clone();
    let discovery = tokio::task::spawn_blocking(move || read_discovery(&paths, limits, &snapshot))
        .await
        .map_err(io::Error::other)?
        .map_err(io::Error::other)?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Coordinator installation requires the active daemon",
            )
        })?;
    let headers = runtime_headers(installation, limits, original).await?;
    let client = reqwest::Client::new();
    let operation = async {
        original.snapshot().check()?;
        let mut response = client
            .post(format!("http://{}{}", discovery.address(), ROUTE))
            .headers(headers.into_iter().collect())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|error| {
                crate::project_hook_install::private_error(
                    io::ErrorKind::Other,
                    "Coordinator installation transport failed",
                    error,
                )
            })?;
        response.error_for_status_ref().map_err(|error| {
            crate::project_hook_install::private_error(
                if matches!(response.status().as_u16(), 401 | 403) {
                    io::ErrorKind::PermissionDenied
                } else {
                    io::ErrorKind::Other
                },
                "Coordinator installation HTTP request rejected",
                error,
            )
        })?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            crate::project_hook_install::private_error(
                io::ErrorKind::Other,
                "Coordinator installation transport failed",
                error,
            )
        })? {
            original.snapshot().check()?;
            if chunk.len()
                > policy
                    .content()
                    .max_output_bytes
                    .saturating_sub(bytes.len())
            {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            bytes.try_reserve(chunk.len()).map_err(io::Error::other)?;
            bytes.extend_from_slice(&chunk);
        }
        let result: CoordinatorOutcome = serde_json::from_slice(&bytes).map_err(|error| {
            crate::project_hook_install::private_error(
                io::ErrorKind::InvalidData,
                "Coordinator installation response invalid",
                error,
            )
        })?;
        original.snapshot().check()?;
        serde_json::to_writer(&mut *output, &result).map_err(io::Error::other)?;
        output.write_all(b"\n")?;
        output.flush()?;
        Ok(if result.failure.is_some() {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        })
    };
    tokio::select! {result=operation=>result,
    ()=original.cancellation().cancelled()=>Err(io::ErrorKind::Interrupted.into()),
    ()=tokio::time::sleep_until(deadline.into())=>Err(io::ErrorKind::TimedOut.into())}
}
