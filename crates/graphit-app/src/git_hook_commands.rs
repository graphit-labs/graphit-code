//! Project commands use Git's shell and the existing process-tree supervisor.
use graphit_core::{
    config::{ConfigKey, ConfigSnapshot},
    file_lock::Cancellation,
    git_hooks::Event,
};
use graphit_hub::policy::HubResourcePolicy;
use graphit_process::{
    HostLimits, HostReadiness, HostShutdown, HostShutdownObservation, InheritedServiceRequest,
    ProcessHost, join_inherited_service, spawn_inherited_service,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::File,
    io::{self, Seek},
    path::Path,
    process::{ExitCode, ExitStatus, Stdio},
    time::{Duration, Instant},
};

pub const CONFIG_KEY: &str = "githooks";

pub fn configured(config: &ConfigSnapshot) -> io::Result<BTreeMap<Event, Vec<String>>> {
    graphit_core::git_hooks::commands(
        config
            .get(&ConfigKey::new(CONFIG_KEY)?)?
            .unwrap_or_default(),
    )
}

pub fn records_enabled(config: &ConfigSnapshot) -> io::Result<bool> {
    use graphit_core::module_settings::{Module, ModuleSettings};
    let modules = ModuleSettings::resolve(config)?;
    Ok(modules.enabled(Module::Task) || modules.enabled(Module::Memory))
}

/// Finite Git payloads are replayed from a private anonymous file, not retained
/// in memory. Interactive proc-receive instead inherits its live bidirectional I/O.
pub const INPUT_COPY_COMMAND: &str = "_git-hook-input";

pub async fn capture_input(
    event: Event,
    cwd: &Path,
    variables: &[(OsString, OsString)],
    policy: &HubResourcePolicy,
    cancellation: &Cancellation,
) -> io::Result<Option<File>> {
    if !matches!(
        event,
        Event::PrePush
            | Event::PreReceive
            | Event::PostReceive
            | Event::PostRewrite
            | Event::ReferenceTransaction
    ) {
        return Ok(None);
    }
    let mut file = tempfile::tempfile()?;
    // std/Tokio stdin reads are blocking and cannot be cancelled while a pipe is
    // open. Supervise this minimal copy in our own executable, without starting
    // another CLI runtime or depending on an external cat/tee utility.
    let stop = Cancellation::default();
    let host = ProcessHost::install(&stop, HostLimits::new(1, 1)?)?;
    let cleanup = Duration::from_secs(3);
    let result = async {
        let request = policy
            .request_context(Instant::now(), cancellation)
            .map_err(io::Error::other)?;
        if !matches!(
            host.ready(request.snapshot().required_deadline()?).await,
            HostReadiness::Ready
        ) {
            return Err(io::Error::other("Git hook stdin process host unavailable"));
        }
        let child = spawn_inherited_service(
            InheritedServiceRequest {
                program: std::env::current_exe()?,
                args: vec![INPUT_COPY_COMMAND.into()],
                cwd: cwd.to_owned(),
                environment: variables.to_vec(),
                stdin: Stdio::inherit(),
                stdout: Stdio::from(file.try_clone()?),
                termination_grace: Duration::from_secs(1),
                cleanup,
            },
            request.snapshot(),
            &host,
        )
        .await
        .map_err(|e| io::Error::other(format!("Git hook stdin could not start: {e:?}")))?;
        let joined = join_inherited_service(child);
        tokio::pin!(joined);
        let outcome = tokio::select! {
            result = &mut joined => result,
            () = cancellation.cancelled() => { stop.cancel(); joined.await },
        }
        .map_err(|e| io::Error::other(format!("Git hook stdin stopped: {e:?}")))?;
        if !outcome.status.success() {
            return Err(io::Error::other(format!(
                "Git hook stdin copy failed: {}",
                outcome.status
            )));
        }
        Ok(ExitCode::SUCCESS)
    }
    .await;
    finish_host(host, &stop, result, cancellation, cleanup).await?;
    if cancellation.is_cancelled() {
        return Err(io::ErrorKind::Interrupted.into());
    }
    file.rewind()?;
    Ok(Some(file))
}

fn exit_status(status: ExitStatus) -> ExitCode {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return ExitCode::from(u8::try_from(128 + signal).unwrap_or(1));
        }
    }
    ExitCode::from(
        status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(1),
    )
}

#[allow(clippy::too_many_arguments)] // Original Git context and process custody remain explicit.
pub async fn execute(
    commands: &[String],
    event: Event,
    arguments: &[String],
    cwd: &Path,
    variables: &[(OsString, OsString)],
    input: Option<&mut File>,
    policy: &HubResourcePolicy,
    cancellation: &Cancellation,
) -> io::Result<ExitCode> {
    if commands.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    let startup = policy
        .request_context(Instant::now(), cancellation)
        .map_err(io::Error::other)?;
    let shell = graphit_core::git_hooks::shell_path(
        cwd,
        startup
            .snapshot()
            .required_deadline()?
            .saturating_duration_since(Instant::now()),
        cancellation,
    )?;
    let stop = Cancellation::default();
    let host = ProcessHost::install(&stop, HostLimits::new(1, 1)?)?;
    let cleanup = Duration::from_secs(3);
    let result = async {
        match host.ready(startup.snapshot().required_deadline()?).await {
            HostReadiness::Ready => (),
            other => {
                return Err(io::Error::other(format!(
                    "Git hook process host unavailable: {other:?}"
                )));
            }
        }
        let mut environment = variables.iter().cloned().collect::<BTreeMap<_, _>>();
        environment.insert("GRAPHIT_HOOK_NAME".into(), event.as_str().into());
        environment.insert(
            "GRAPHIT_HOOK_ARG_COUNT".into(),
            arguments.len().to_string().into(),
        );
        // A parent hook can invoke another Git event. Do not leak its positional slots.
        environment.retain(|key, _| {
            !key.to_str().is_some_and(|key| {
                key.strip_prefix("GRAPHIT_HOOK_ARG_").is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
                })
            })
        });
        for (index, value) in arguments.iter().enumerate() {
            environment.insert(
                format!("GRAPHIT_HOOK_ARG_{}", index + 1).into(),
                value.into(),
            );
        }
        let mut input = input;
        for (index, command) in commands.iter().enumerate() {
            if cancellation.is_cancelled() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let stdin = match input.as_deref_mut() {
                Some(file) => {
                    file.rewind()?;
                    Stdio::from(file.try_clone()?)
                }
                None => Stdio::inherit(),
            };
            let args = [
                OsString::from("-c"),
                command.into(),
                OsString::from("graphit-git-hook"),
            ]
            .into_iter()
            .chain(arguments.iter().map(OsString::from))
            .collect();
            eprintln!("Git {} [{}]: {command}", event.as_str(), index + 1);
            // Only admission uses the finite request deadline. The hook itself can
            // run as long as needed; its lifetime is cancellation, not an RPC timeout.
            let admission = policy
                .request_context(Instant::now(), cancellation)
                .map_err(io::Error::other)?;
            let process = spawn_inherited_service(
                InheritedServiceRequest {
                    program: shell.clone(),
                    args,
                    cwd: cwd.to_owned(),
                    environment: environment
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                    stdin,
                    stdout: Stdio::inherit(),
                    termination_grace: Duration::from_secs(1),
                    cleanup,
                },
                admission.snapshot(),
                &host,
            )
            .await
            .map_err(|e| {
                io::Error::other(format!(
                    "Git {} command {} could not start: {e:?}",
                    event.as_str(),
                    index + 1
                ))
            })?;
            let joined = join_inherited_service(process);
            tokio::pin!(joined);
            let outcome = tokio::select! {
                result = &mut joined => result,
                () = cancellation.cancelled() => { stop.cancel(); joined.await },
            }
            .map_err(|e| {
                if cancellation.is_cancelled() {
                    io::Error::from(io::ErrorKind::Interrupted)
                } else {
                    io::Error::other(format!(
                        "Git {} command {} stopped: {e:?}",
                        event.as_str(),
                        index + 1
                    ))
                }
            })?;
            if !outcome.status.success() {
                eprintln!(
                    "Git {} command {} failed ({}): {command}",
                    event.as_str(),
                    index + 1,
                    outcome.status
                );
                return Ok(exit_status(outcome.status));
            }
        }
        Ok(ExitCode::SUCCESS)
    }
    .await;
    finish_host(host, &stop, result, cancellation, cleanup).await
}

async fn finish_host(
    host: ProcessHost,
    stop: &Cancellation,
    result: io::Result<ExitCode>,
    cancellation: &Cancellation,
    cleanup: Duration,
) -> io::Result<ExitCode> {
    stop.cancel();
    let shutdown = match host.shutdown(Instant::now() + cleanup).await {
        HostShutdown::Pending(mut pending) => {
            match pending.repair(Instant::now() + cleanup).await {
                HostShutdownObservation::Joined(report) => HostShutdown::Joined(report),
                HostShutdownObservation::Pending => HostShutdown::Pending(pending),
            }
        }
        joined => joined,
    };
    match shutdown {
        HostShutdown::Joined(report) if report.failure().is_none() => match result {
            Err(_) if cancellation.is_cancelled() => Ok(ExitCode::from(130)),
            other => other,
        },
        other => Err(io::Error::other(format!(
            "Git hook process cleanup failed: {other:?}"
        ))),
    }
}
