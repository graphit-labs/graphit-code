//! Explicit CLI installation through the active Rust daemon.
use super::{HookOperation, HookOutcome, HookRequest, ROUTE};
use crate::ast_source::runtime_headers;
use graphit_core::project_paths::{InstallationPaths, ProjectPaths};
use graphit_daemon::source_discovery::read_discovery;
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, RequestContext},
};
use std::{
    io::{self, Write},
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
    /// Remove owned hooks and MCP, preserving Kimi references from other projects.
    Remove(Selection),
}
#[derive(clap::Args)]
pub struct Selection {
    /// Native agent schema selected explicitly.
    #[arg(long, value_parser = parse_binding)]
    pub binding: NativeBinding,
    /// Existing Coordinator Markdown filename; required only for Kiro.
    #[arg(long)]
    pub profile: Option<String>,
}
pub(crate) fn parse_binding(value: &str) -> Result<NativeBinding, String> {
    NativeBinding::ALL
        .into_iter()
        .find(|binding| binding.as_str() == value)
        .ok_or_else(|| {
            format!(
                "native binding must be one of: {}",
                NativeBinding::ALL.map(NativeBinding::as_str).join(", ")
            )
        })
}

/// Names forwarded by the native host; values belong only to the launching process.
pub fn mcp_environment_variables(
    brand: &graphit_core::brand::Brand,
    binding: NativeBinding,
) -> io::Result<Vec<String>> {
    if binding != NativeBinding::Codex {
        return Ok(Vec::new());
    }
    let mut variables = vec![brand.environment_variable("GLOBAL_DIR")?];
    variables.extend(
        [
            crate::role_proxy::BEARER_ENV,
            crate::role_proxy::REFERENCE_ENV,
            crate::role_proxy::SELECTION_ENV,
            crate::role_proxy::BINDING_ENV,
            crate::browser_auth::live_mcp::URL_ENV,
            crate::browser_auth::live_mcp::BEARER_ENV,
            crate::browser_auth::live_mcp::SESSION_ENV,
        ]
        .map(str::to_owned),
    );
    Ok(variables)
}

/// Build the exact program and argv published by native consumers.
pub fn native_invocation(
    installation: &InstallationPaths,
    project: Option<&ProjectPaths>,
    version: Option<&str>,
    engine: crate::session_hook_entry::Engine,
    event: crate::session_hook_entry::Event,
    recipient: crate::session_hook_entry::Recipient,
) -> io::Result<std::process::Command> {
    use clap::ValueEnum;
    event
        .semantic()
        .native_name(engine.binding())
        .map_err(io::Error::other)?;
    let engine = engine
        .to_possible_value()
        .expect("closed native host selection");
    let event = event
        .to_possible_value()
        .expect("closed native event selection");
    let mut command = std::process::Command::new(installation.brand().binary_name());
    if let Some(version) = version {
        command.args(["--project-version", version]);
    }
    command.args([
        "_session-hook",
        "--engine",
        engine.get_name(),
        "--event",
        event.get_name(),
    ]);
    if project.is_some() {
        command.args(["--project-dir", "."]);
    }
    if recipient == crate::session_hook_entry::Recipient::Role {
        command.args(["--recipient", "role"]);
    }
    Ok(command)
}

/// Serialize the canonical invocation for a command-hook field.
/// Windows literal tokens need no shell-specific quoting; other arguments remain
/// unavailable until the consuming host's quoting contract is qualified.
pub fn native_command(
    installation: &InstallationPaths,
    project: Option<&ProjectPaths>,
    version: Option<&str>,
    engine: crate::session_hook_entry::Engine,
    event: crate::session_hook_entry::Event,
    recipient: crate::session_hook_entry::Recipient,
) -> io::Result<String> {
    let command = native_invocation(installation, project, version, engine, event, recipient)?;
    let argv = std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|argument| argument.to_str().ok_or(io::ErrorKind::InvalidInput))
        .collect::<Result<Vec<_>, _>>()?;
    if cfg!(windows) {
        literal_command(&argv)
    } else {
        shlex::try_join(argv).map_err(|_| io::ErrorKind::InvalidInput.into())
    }
}

fn literal_command(argv: &[&str]) -> io::Result<String> {
    if argv.iter().any(|argument| {
        argument.is_empty()
            || !argument.chars().all(|c| {
                c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '+')
            })
    }) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows command-hook quoting is not qualified for this argument; use a native executable-and-argv integration or a qualified command-hook shell",
        ));
    }
    Ok(argv.join(" "))
}

pub(crate) fn kimi_config_dir(binding: NativeBinding) -> io::Result<Option<std::path::PathBuf>> {
    if binding != NativeBinding::Kimi {
        return Ok(None);
    }
    let path = match std::env::var_os("KIMI_CODE_HOME") {
        Some(path) => std::path::PathBuf::from(path),
        None => std::env::home_dir()
            .ok_or(io::ErrorKind::NotFound)?
            .join(".kimi-code"),
    };
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(Some(path))
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
            "Root hook installation is unavailable in a managed-role transport context",
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
    original.snapshot().required_deadline()?;
    root_transport()?;
    let (operation, selection) = match arguments.command {
        Command::Install(selection) => (HookOperation::Install, selection),
        Command::Remove(selection) => (HookOperation::Remove, selection),
    };
    let input = HookRequest {
        operation,
        binding: selection.binding,
        profile: selection.profile,
        kimi_config_dir: kimi_config_dir(selection.binding)?,
        project_dir: project.root().into(),
        project_version: version,
    };
    input.validate()?;
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
    let max_bytes = policy.content().max_input_bytes;
    let paths = installation.clone();
    let snapshot = original.snapshot().clone();
    let discovery =
        tokio::task::spawn_blocking(move || read_discovery(&paths, max_bytes, &snapshot))
            .await
            .map_err(io::Error::other)?
            .map_err(io::Error::other)?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "hook installation requires the active daemon",
                )
            })?;
    let headers = runtime_headers(installation, original).await?;
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
                super::private_error(
                    io::ErrorKind::Other,
                    "hook installation transport failed",
                    error,
                )
            })?;
        response.error_for_status_ref().map_err(|error| {
            super::private_error(
                if matches!(response.status().as_u16(), 401 | 403) {
                    io::ErrorKind::PermissionDenied
                } else {
                    io::ErrorKind::Other
                },
                "hook installation HTTP request rejected",
                error,
            )
        })?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            super::private_error(
                io::ErrorKind::Other,
                "hook installation transport failed",
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
        let result: HookOutcome = serde_json::from_slice(&bytes).map_err(|error| {
            super::private_error(
                io::ErrorKind::InvalidData,
                "hook installation response invalid",
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

#[cfg(test)]
mod command_tests {
    use super::*;
    use crate::session_hook_entry::{Engine, Event, Recipient};

    #[test]
    fn native_hook_keeps_literal_explicit_version_and_relative_project() {
        let temp = tempfile::tempdir().unwrap();
        let installation = InstallationPaths::resolve(
            graphit_core::brand::Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let project = ProjectPaths::at(temp.path(), &installation).unwrap();
        let command = native_command(
            &installation,
            Some(&project),
            Some("feature/windows"),
            Engine::Codex,
            Event::SessionStart,
            Recipient::Role,
        )
        .unwrap();
        assert_eq!(
            command,
            "graphit --project-version feature/windows _session-hook --engine codex --event SessionStart --project-dir . --recipient role",
        );
    }

    #[test]
    fn unqualified_windows_hook_quoting_reports_capability_without_values() {
        for argument in [
            "",
            "two words",
            "quote'",
            "quote\"",
            "$secret",
            "a;b",
            "a&b",
            "a|b",
            "%secret%",
            "!secret!",
            "a^b",
            "a`b",
            "(secret)",
            "a\nb",
        ] {
            let error = literal_command(&["graphit", "--project-version", argument]).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::Unsupported);
            assert_eq!(
                error.to_string(),
                "Windows command-hook quoting is not qualified for this argument; use a native executable-and-argv integration or a qualified command-hook shell",
            );
        }
        assert_eq!(
            literal_command(&["graphit", "--project-version", "versão/α+1.2"]).unwrap(),
            "graphit --project-version versão/α+1.2",
        );
    }

    #[cfg(windows)]
    #[test]
    fn literal_hook_tokens_cross_native_windows_shells() {
        use std::os::windows::process::CommandExt;

        let root = std::path::PathBuf::from(
            std::env::var_os("SYSTEMROOT").expect("Windows shell test requires SYSTEMROOT"),
        );
        let system = root.join("System32");
        let cmd = system.join("cmd.exe");
        let temp = tempfile::tempdir().unwrap();
        let installation = InstallationPaths::resolve(
            graphit_core::brand::Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let project = ProjectPaths::at(temp.path(), &installation).unwrap();
        let arguments = [
            "--project-version",
            "feature/windows",
            "_session-hook",
            "--engine",
            "codex",
            "--event",
            "SessionStart",
            "--project-dir",
            ".",
            "--recipient",
            "role",
        ];
        let command = native_command(
            &installation,
            Some(&project),
            Some("feature/windows"),
            Engine::Codex,
            Event::SessionStart,
            Recipient::Role,
        )
        .unwrap();
        let fixture = std::path::PathBuf::from(
            std::env::var_os("GRAPHIT_TEST_APP_AGENT_FIXTURE")
                .expect("cargo xtask cargo test must emit the app-agent-fixture target"),
        );
        assert!(fixture.is_absolute() && fixture.is_file());
        let executable = temp
            .path()
            .join(format!("{}.exe", installation.brand().binary_name()));
        std::fs::copy(&fixture, &executable).unwrap();
        std::fs::write(
            executable.with_extension("fixture.json"),
            serde_json::to_vec(&serde_json::json!({
                "kind": "raw",
                "process": {"args": arguments, "stdout": arguments.join(" ")},
            }))
            .unwrap(),
        )
        .unwrap();
        let path = std::env::join_paths([
            temp.path(),
            fixture.parent().expect("fixture executable parent"),
            system.as_path(),
        ])
        .unwrap();
        for (program, options) in [
            (cmd.clone(), ["/D", "/S", "/C"]),
            (
                system.join("WindowsPowerShell/v1.0/powershell.exe"),
                ["-NoProfile", "-NonInteractive", "-Command"],
            ),
        ] {
            let output = std::process::Command::new(&program)
                .args(options)
                .raw_arg(&command)
                .env_clear()
                .env("SYSTEMROOT", &root)
                .env("WINDIR", &root)
                .env("COMSPEC", &cmd)
                .env("PATHEXT", ".EXE")
                .env("TEMP", temp.path())
                .env("TMP", temp.path())
                .env("PATH", &path)
                .current_dir(temp.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "native shell {} failed ({}): {}",
                program.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr),
            );
            assert!(output.stderr.is_empty());
            assert_eq!(
                String::from_utf8(output.stdout).unwrap().trim(),
                arguments.join(" "),
            );
        }
    }
}
