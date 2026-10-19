//! Open the configured Browser through the existing authenticated daemon control.
use crate::{daemon_control, daemon_launcher, service_entry::ServiceCommand};
use graphit_hub::policy::RequestContext;
use graphit_process::ProcessHost;
use std::{fmt, io, sync::Arc};

#[derive(clap::Args, Debug)]
pub struct Arguments {
    /// Print the configured Browser URL without launching a desktop application.
    #[arg(long)]
    pub no_open: bool,
}

#[derive(Debug)]
pub enum Failure {
    Launcher(Box<daemon_launcher::Failure>),
    Control(io::Error),
    Desktop { url: String, cause: DesktopFailure },
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Launcher(_) => f.write_str("UI daemon startup failed"),
            Self::Control(error) => write!(f, "UI control failed: {error}"),
            Self::Desktop { url, cause } => {
                write!(f, "desktop opener failed ({cause}); open {url} manually")
            }
        }
    }
}
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Launcher(error) => Some(error.as_ref()),
            Self::Control(error) => Some(error),
            Self::Desktop { .. } => None,
        }
    }
}
#[derive(Debug)]
pub enum DesktopFailure {
    Launch(io::Error),
    Worker(tokio::task::JoinError),
    Input(io::Error),
}

impl fmt::Display for DesktopFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Launch(error) => write!(f, "opener launch failed: {error}"),
            Self::Worker(error) => write!(f, "opener worker failed: {error}"),
            Self::Input(error) => write!(f, "opener input failed: {error}"),
        }
    }
}

pub async fn execute(
    host: &ProcessHost,
    inputs: daemon_launcher::Inputs<'_>,
    original: &RequestContext<'_>,
    arguments: Arguments,
) -> Result<String, Failure> {
    let running = daemon_launcher::execute(
        host,
        daemon_launcher::Inputs {
            installation: inputs.installation,
            executable: inputs.executable,
            environment: inputs.environment,
            client: inputs.client,
            limits: inputs.limits,
            termination_grace: inputs.termination_grace,
            cleanup: inputs.cleanup,
        },
        original,
        &ServiceCommand::Start,
    )
    .await
    .map_err(|e| Failure::Launcher(Box::new(e)))?;
    if !matches!(running, daemon_launcher::Outcome::Running(_)) {
        return Err(Failure::Control(io::Error::new(
            io::ErrorKind::NotConnected,
            "daemon did not acknowledge readiness",
        )));
    }
    let reply =
        daemon_launcher::control(&inputs, original, daemon_control::Operation::OpenUi, None)
            .await
            .map_err(Failure::Control)?
            .ok_or_else(|| Failure::Control(io::ErrorKind::NotConnected.into()))?;
    let url = reply
        .ui_url
        .ok_or_else(|| Failure::Control(io::ErrorKind::InvalidData.into()))?;
    daemon_control::validate_ui_url(&url).map_err(Failure::Control)?;
    if !arguments.no_open {
        open(&url, original)
            .await
            .map_err(|cause| Failure::Desktop {
                url: url.clone(),
                cause,
            })?;
    }
    original.check().map_err(Failure::Control)?;
    Ok(url)
}

/// Transfer the URL to the desktop; its application outlives this request.
pub(crate) async fn open(url: &str, original: &RequestContext<'_>) -> Result<(), DesktopFailure> {
    let parsed =
        url::Url::parse(url).map_err(|error| DesktopFailure::Input(io::Error::other(error)))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(DesktopFailure::Input(io::Error::new(
            io::ErrorKind::InvalidInput,
            "desktop opener requires a web URL without credentials",
        )));
    }
    original.check().map_err(DesktopFailure::Input)?;
    let snapshot = Arc::clone(original.snapshot());
    let target = url.to_owned();
    tokio::task::spawn_blocking(move || {
        snapshot.check().map_err(DesktopFailure::Input)?;
        // Uses the process's desktop environment and OS handler, with no supervised
        // process group that would terminate the browser after its launcher exits.
        open::that_detached(target).map_err(DesktopFailure::Launch)
    })
    .await
    .map_err(DesktopFailure::Worker)?
}
