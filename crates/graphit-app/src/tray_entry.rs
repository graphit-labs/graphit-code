//! Linux desktop consumer of the existing native daemon and Auth controls.
mod linux;

use crate::{auth_active, auth_cli, daemon_launcher, service_entry::ServiceCommand, ui_entry};
use graphit_core::file_lock::Cancellation;
use graphit_hub::policy::HubResourcePolicy;
use graphit_process::ProcessHost;
use ksni::TrayMethods;
use std::{fmt, io, process::ExitCode, time::Instant};
use tokio::sync::mpsc;

#[derive(Clone)]
struct LoginProfile {
    provider: String,
    name: String,
    s3_disabled: bool,
}

#[derive(Clone)]
enum Action {
    Refresh,
    Start,
    Restart,
    OpenUi,
    Login(LoginProfile),
    Quit,
}

#[derive(Debug)]
pub enum Failure {
    Io(io::Error),
    Request(Box<graphit_hub::policy::RequestError>),
    Desktop(ksni::Error),
    Launcher(Box<daemon_launcher::Failure>),
    OpenUi(Box<ui_entry::Failure>),
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "tray: {error}"),
            Self::Request(error) => write!(f, "tray request: {error}"),
            Self::Desktop(error) => write!(f, "tray desktop unavailable: {error}"),
            Self::Launcher(error) => error.fmt(f),
            Self::OpenUi(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Io(error) => error,
            Self::Request(error) => error.as_ref(),
            Self::Desktop(error) => error,
            Self::Launcher(error) => error.as_ref(),
            Self::OpenUi(error) => error.as_ref(),
        })
    }
}
impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

struct Update {
    state: String,
    running: bool,
    profiles: Option<Vec<LoginProfile>>,
}

pub async fn execute(
    host: &ProcessHost,
    inputs: daemon_launcher::Inputs<'_>,
    policy: &HubResourcePolicy,
    shutdown: &Cancellation,
) -> Result<(), Failure> {
    let (sender, mut actions) = mpsc::unbounded_channel();
    let (announce, mut announcements) = mpsc::unbounded_channel::<String>();
    let tray = linux::Tray {
        name: inputs.installation.brand().binary_name().to_owned(),
        state: "Checking daemon".into(),
        error: None,
        profiles: Vec::new(),
        busy: false,
        quitting: false,
        running: false,
        sender,
    };
    let handle = tray
        .assume_sni_available(true)
        .spawn()
        .await
        .map_err(Failure::Desktop)?;
    let mut next = Some(Action::Refresh);
    let mut pending = None;
    let result = loop {
        let action = match next.take() {
            Some(action) => action,
            None => tokio::select! {
                () = shutdown.cancelled() => break Ok(()),
                action = actions.recv() => match action {Some(action) => action, None => break Ok(())},
            },
        };
        let quit = matches!(action, Action::Quit);
        let refresh = matches!(action, Action::Refresh);
        let cancel = Cancellation::default();
        let original = match policy.request_context(Instant::now(), &cancel) {
            Ok(original) => original,
            Err(error) => break Err(Failure::Request(Box::new(error))),
        };
        // Keep provisional startup custody until the existing reaper has joined.
        if let Some(error) = pending.as_mut()
            && let Err(cause) = repair_launcher(error, Instant::now() + inputs.cleanup).await
        {
            handle
                .update(|tray| {
                    tray.busy = false;
                    tray.quitting = false;
                    tray.error = Some(cause.to_string())
                })
                .await;
            continue;
        }
        pending.take();
        handle
            .update(|tray| {
                if !refresh {
                    tray.busy = true;
                    tray.quitting = quit;
                    tray.error = None;
                    tray.state = if quit { "Stopping daemon" } else { "Working" }.into();
                }
            })
            .await;
        let work = perform(host, &inputs, policy, &original, action, &announce);
        tokio::pin!(work);
        let mut stopping = false;
        let mut opener_failure = None;
        let completed = loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled(), if !stopping => {stopping=true; cancel.cancel();}
                requested = actions.recv(), if !stopping => {
                    match requested {
                        Some(Action::Refresh) => {},
                        Some(action) if refresh => {
                            // Refresh only reads status/profiles. Let an explicit action
                            // replace it instead of consuming the user's menu click.
                            cancel.cancel();
                            next = Some(action);
                            break None;
                        }
                        Some(Action::Quit) if !quit => {cancel.cancel();next=Some(Action::Quit);}
                        None => {stopping=true;cancel.cancel();}
                        _ => {},
                    }
                }
                url = announcements.recv(), if !stopping => {
                    if let Some(url) = url {
                        let opening = ui_entry::open(&url, &original);
                        tokio::pin!(opening);
                        let opened = tokio::select! {
                            result=&mut opening => result,
                            ()=shutdown.cancelled()=>{stopping=true;cancel.cancel();opening.await},
                            requested=actions.recv()=>{
                                cancel.cancel();
                                if matches!(requested,Some(Action::Quit)){next=Some(Action::Quit);}else if requested.is_none(){stopping=true;}
                                opening.await
                            }
                        };
                        if let Err(error)=opened {opener_failure=Some(format!("desktop opener failed: {error}"));cancel.cancel();}
                    }
                }
                result = &mut work => break Some(result),
            }
        };
        let Some(completed) = completed else { continue };
        if stopping {
            break completed.map(|_| ());
        }
        match completed {
            Ok(update) => {
                handle
                    .update(|tray| {
                        tray.state = update.state;
                        tray.running = update.running;
                        tray.error = None;
                        if !refresh {
                            tray.busy = false;
                            tray.quitting = false;
                        }
                        if let Some(profiles) = update.profiles {
                            tray.profiles = profiles;
                        }
                    })
                    .await;
                if quit {
                    break Ok(());
                }
            }
            Err(mut error) => {
                if let Err(cleanup) =
                    repair_launcher(&mut error, Instant::now() + inputs.cleanup).await
                {
                    handle
                        .update(|tray| tray.error = Some(format!("{error}; {cleanup}")))
                        .await;
                    pending = Some(error);
                } else {
                    handle
                        .update(|tray| {
                            tray.error = Some(opener_failure.unwrap_or_else(|| error.to_string()))
                        })
                        .await;
                }
                handle
                    .update(|tray| {
                        tray.state = "Action failed".into();
                        if !refresh {
                            tray.busy = false;
                            tray.quitting = false;
                        }
                    })
                    .await;
            }
        }
        while announcements.try_recv().is_ok() {}
    };
    handle.shutdown().await;
    if let Some(mut pending) = pending
        && repair_launcher(&mut pending, Instant::now() + inputs.cleanup)
            .await
            .is_err()
    {
        return Err(pending);
    }
    result
}

async fn perform(
    host: &ProcessHost,
    inputs: &daemon_launcher::Inputs<'_>,
    policy: &HubResourcePolicy,
    original: &graphit_hub::policy::RequestContext<'_>,
    action: Action,
    announce: &mpsc::UnboundedSender<String>,
) -> Result<Update, Failure> {
    let launcher = || daemon_launcher::Inputs {
        installation: inputs.installation,
        executable: inputs.executable,
        environment: inputs.environment,
        client: inputs.client,
        limits: inputs.limits,
        termination_grace: inputs.termination_grace,
        cleanup: inputs.cleanup,
    };
    if matches!(action, Action::OpenUi) {
        ui_entry::execute(
            host,
            launcher(),
            original,
            ui_entry::Arguments { no_open: false },
        )
        .await
        .map_err(|error| Failure::OpenUi(Box::new(error)))?;
    }
    let command = match action {
        Action::Start | Action::Login(_) => ServiceCommand::Start,
        Action::Restart => ServiceCommand::Restart,
        Action::Quit => ServiceCommand::Stop,
        Action::Refresh | Action::OpenUi => ServiceCommand::Status,
    };
    let outcome = if matches!(command, ServiceCommand::Status) {
        match daemon_launcher::control(
            inputs,
            original,
            crate::daemon_control::Operation::Status,
            None,
        )
        .await?
        {
            Some(reply) => daemon_launcher::Outcome::Running(reply),
            None => daemon_launcher::Outcome::Absent,
        }
    } else {
        daemon_launcher::execute(host, launcher(), original, &command)
            .await
            .map_err(|error| Failure::Launcher(Box::new(error)))?
    };
    let running = matches!(outcome, daemon_launcher::Outcome::Running(_));
    let state = match outcome {
        daemon_launcher::Outcome::Running(reply) => format!("Daemon running (PID {})", reply.pid),
        daemon_launcher::Outcome::Absent | daemon_launcher::Outcome::Stopped => {
            "Daemon stopped".into()
        }
        other => format!("Daemon {other:?}"),
    };
    if let Action::Login(profile) = action {
        let arguments = auth_cli::LoginArguments {
            device: false,
            profile: profile.name,
            provider: profile.provider,
            username: None,
            organization: None,
            teams: Vec::new(),
            anonymous: false,
            s3_disabled: profile.s3_disabled,
            mcp_key: None,
            broker_key: None,
            embedding_api_key: None,
            rerank_api_key: None,
            s3_access_key: None,
            s3_secret_key: None,
            s3_session_token: None,
            s3_expires_at: None,
            aws_profile: None,
            expected_revision: None,
        };
        auth(
            inputs,
            policy,
            original,
            &auth_cli::Command::Login(Box::new(arguments)),
            announce,
        )
        .await?;
    }
    let profiles = if running {
        let providers = auth(
            inputs,
            policy,
            original,
            &auth_cli::Command::Provider(Box::new(auth_cli::ProviderArguments {
                command: auth_cli::ProviderCommand::List,
            })),
            announce,
        )
        .await?;
        let accounts = auth(
            inputs,
            policy,
            original,
            &auth_cli::Command::Account(auth_cli::AccountArguments {
                command: auth_cli::AccountCommand::List,
            }),
            announce,
        )
        .await?;
        let providers: serde_json::Value =
            serde_json::from_slice(&providers).map_err(io::Error::other)?;
        let accounts: serde_json::Value =
            serde_json::from_slice(&accounts).map_err(io::Error::other)?;
        let broker_names = providers["providers"]
            .as_array()
            .ok_or_else(|| io::Error::other("Auth providers missing"))?
            .iter()
            .filter(|p| p["config"]["type"] == "broker")
            .filter_map(|p| p["name"].as_str())
            .collect::<Vec<_>>();
        Some(
            accounts["profiles"]
                .as_array()
                .ok_or_else(|| io::Error::other("Auth profiles missing"))?
                .iter()
                .filter_map(|p| {
                    let name = p["name"].as_str()?;
                    let provider = p["provider"].as_str()?;
                    let s3_disabled = p["s3_disabled"].as_bool()?;
                    broker_names.contains(&provider).then(|| LoginProfile {
                        name: name.into(),
                        provider: provider.into(),
                        s3_disabled,
                    })
                })
                .collect(),
        )
    } else {
        Some(Vec::new())
    };
    original.check()?;
    Ok(Update {
        state,
        running,
        profiles,
    })
}

async fn auth(
    inputs: &daemon_launcher::Inputs<'_>,
    policy: &HubResourcePolicy,
    original: &graphit_hub::policy::RequestContext<'_>,
    command: &auth_cli::Command,
    announce: &mpsc::UnboundedSender<String>,
) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut diagnostic = Vec::new();
    let result = auth_active::run_if_active(
        command,
        inputs.installation,
        inputs.client,
        inputs.limits,
        policy.content().max_output_bytes,
        original,
        &mut output,
        &mut diagnostic,
        |url| {
            announce
                .send(url.to_owned())
                .map_err(|_| io::Error::from(io::ErrorKind::Interrupted))
        },
    )
    .await?;
    if result != Some(ExitCode::SUCCESS) {
        return Err(io::Error::other("active Auth command failed"));
    }
    Ok(output)
}

async fn repair_launcher(error: &mut Failure, deadline: Instant) -> io::Result<()> {
    let launcher = match error {
        Failure::Launcher(error) => Some(error.as_mut()),
        Failure::OpenUi(error) => match error.as_mut() {
            ui_entry::Failure::Launcher(error) => Some(error.as_mut()),
            _ => None,
        },
        _ => None,
    };
    let remaining = match launcher {
        Some(daemon_launcher::Failure::Detached(error)) => &mut error.remaining,
        Some(daemon_launcher::Failure::Startup {
            cleanup: Some(error),
            ..
        }) => &mut error.remaining,
        _ => return Ok(()),
    };
    if let Some(owner) = remaining {
        if owner.repair(deadline).await?.is_none() {
            return Err(io::Error::other("daemon startup cleanup pending"));
        }
        remaining.take();
    }
    Ok(())
}
