//! Typed CLI operations on the already composed per-user service owner.
use graphit_core::file_io::SnapshotContext;
use graphit_daemon::{control::UserService, services::ServiceFailure};
use graphit_process::Outcome;
use std::{io, sync::Arc};

/// Flatten these operations into the central `daemon` parser.
#[derive(Clone, Debug, clap::Subcommand)]
pub enum ServiceCommand {
    /// Start the user's daemon for this installation.
    Start,
    /// Stop the user manager before draining the daemon's retained owners.
    Stop,
    /// Restart the registered service with its native user manager.
    Restart,
    /// Observe the authenticated daemon or its native manager.
    Status,
    /// Register the service; login startup is enabled only with --login.
    Install {
        #[arg(long)]
        login: bool,
    },
    /// Change startup at user login explicitly.
    Login {
        #[command(subcommand)]
        action: LoginAction,
    },
    /// Stop and remove the service and its login registration.
    Uninstall,
}

#[derive(Clone, Debug, clap::Subcommand)]
pub enum LoginAction {
    Enable,
    Disable,
}

pub enum ServiceResult {
    Completed,
    Status(Outcome),
}

/// The caller owns the ProcessHost, native paths and daemon lifecycle.
/// Manager unavailability must be observed by that composition before selecting
/// detached startup; command failure here never authorizes another launch.
pub async fn execute(
    service: &UserService<'_>,
    command: &ServiceCommand,
    original: &Arc<SnapshotContext>,
) -> Result<ServiceResult, ServiceFailure> {
    original.check().map_err(io::Error::other)?;
    match command {
        ServiceCommand::Start => service.start(original).await?,
        ServiceCommand::Stop => service.stop(original).await?,
        ServiceCommand::Restart => service.restart(original).await?,
        ServiceCommand::Status => {
            return service.status(original).await.map(ServiceResult::Status);
        }
        ServiceCommand::Install { login } => service.install(original, *login).await?,
        ServiceCommand::Login { action } => {
            service
                .set_login(original, matches!(action, LoginAction::Enable))
                .await?;
        }
        ServiceCommand::Uninstall => service.uninstall(original).await?,
    }
    Ok(ServiceResult::Completed)
}
